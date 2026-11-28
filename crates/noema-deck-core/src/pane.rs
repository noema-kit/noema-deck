//! Shell panes: each runs one command on a pseudoterminal, so an agent's own
//! interface works inside Deck as it does in any terminal.

mod environment;
mod output;
mod request;

use std::{
    collections::HashMap,
    env,
    io::{Read, Write},
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicU32, Ordering},
    },
    thread,
    time::Duration,
};

use anyhow::{Context, Result, anyhow, ensure};
use parking_lot::Mutex;
use portable_pty::{Child, ChildKiller, CommandBuilder, MasterPty, PtySize, native_pty_system};
use serde::{Deserialize, Serialize};

pub(crate) use self::environment::login as login_environment;
use self::output::Output;
pub use self::request::{Reply, Request};
use crate::agent::Launch;

/// The command that runs the user's own shell in `folder`, as a new terminal
/// tab would.
pub(crate) fn shell(folder: &Path) -> Result<Launch> {
    ensure!(folder.is_dir(), "{} is not a folder", folder.display());
    let login = login_environment().and_then(|environment| {
        environment.iter().find_map(|(name, value)| (name == "SHELL").then(|| value.clone()))
    });
    let program = login
        .or_else(|| env::var("SHELL").ok())
        .filter(|shell| !shell.is_empty())
        .unwrap_or_else(|| "/bin/sh".into());
    Ok(Launch { folder: folder.into(), program, args: vec!["-l".into()] })
}

/// A pane's identity for as long as Deck runs.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct PaneId(u32);

impl PaneId {
    #[cfg(test)]
    pub(crate) const fn new(id: u32) -> Self {
        Self(id)
    }
}

/// Every open pane.
#[derive(Default)]
pub struct Panes {
    last: AtomicU32,
    open: Mutex<HashMap<PaneId, Pane>>,
}

struct Pane {
    master: Box<dyn MasterPty + Send>,
    /// A program that stops reading leaves a write waiting, so each pane's
    /// writes go through a lock of its own, taken after the panes' lock is let go.
    writer: Arc<Mutex<Box<dyn Write + Send>>>,
    killer: Box<dyn ChildKiller + Send + Sync>,
    output: Arc<Output>,
    /// The process the pane runs, when the system names it.
    process: Option<u32>,
}

impl Panes {
    /// Runs `launch` on a new pseudoterminal, with `environment` instead of
    /// this process's own when one is given.
    pub fn open(
        &self,
        launch: &Launch,
        environment: Option<&[(String, String)]>,
        rows: u16,
        cols: u16,
    ) -> Result<PaneId> {
        let pair = native_pty_system().openpty(size(rows, cols)).map_err(|error| anyhow!(error))?;
        let child = pair
            .slave
            .spawn_command(command(launch, environment))
            .map_err(|error| anyhow!(error))
            .with_context(|| format!("Run `{}` in {}", launch.program, launch.folder.display()))?;
        drop(pair.slave);

        let output = Arc::new(Output::default());
        let pane = Pane {
            writer: Arc::new(Mutex::new(
                pair.master.take_writer().map_err(|error| anyhow!(error))?,
            )),
            killer: child.clone_killer(),
            output: Arc::clone(&output),
            process: child.process_id(),
            master: pair.master,
        };
        let reader = pane.master.try_clone_reader().map_err(|error| anyhow!(error))?;
        follow(reader, child, output);

        let id = PaneId(self.last.fetch_add(1, Ordering::Relaxed) + 1);
        self.open.lock().insert(id, pane);
        Ok(id)
    }

    /// Waits up to `wait` for output. Answers with the text so far and whether
    /// the process has ended with nothing left to read.
    pub fn read(&self, pane: PaneId, wait: Duration) -> Result<(String, bool)> {
        // The pane is looked up and let go first, so a wait blocks no other pane.
        let output = self.with(pane, |pane| Ok(Arc::clone(&pane.output)))?;
        Ok(output.take(wait))
    }

    pub fn write(&self, pane: PaneId, data: &[u8]) -> Result<()> {
        let writer = self.with(pane, |pane| Ok(Arc::clone(&pane.writer)))?;
        Ok(writer.lock().write_all(data)?)
    }

    pub fn resize(&self, pane: PaneId, rows: u16, cols: u16) -> Result<()> {
        self.with(pane, |pane| pane.master.resize(size(rows, cols)).map_err(|error| anyhow!(error)))
    }

    /// Stops the pane's process and forgets the pane. Letting go of a
    /// terminal can wait on it, so that happens on a thread of its own.
    pub fn close(&self, pane: PaneId) {
        if let Some(pane) = self.open.lock().remove(&pane) {
            thread::spawn(move || drop(pane));
        }
    }

    /// The process each open pane runs, which everything started in the pane
    /// descends from. A pane whose process has ended has none: the system
    /// may give its number to another.
    pub fn processes(&self) -> Vec<(PaneId, u32)> {
        let open = self.open.lock();
        let running = open.iter().filter(|(_, pane)| !pane.output.ended());
        running.filter_map(|(id, pane)| Some((*id, pane.process?))).collect()
    }

    fn with<T>(&self, pane: PaneId, act: impl FnOnce(&mut Pane) -> Result<T>) -> Result<T> {
        act(self.open.lock().get_mut(&pane).context("That pane is closed")?)
    }
}

impl Drop for Pane {
    fn drop(&mut self) {
        let _ = self.killer.kill();
        self.output.end();
    }
}

/// Whether a variable names the terminal Deck itself was started from, such
/// as iTerm2 when started with `cargo run` there. A pane passing it on would
/// claim to be that terminal, and a program would speak to it in that
/// terminal's dialect: Neovim told it runs in iTerm2 writes colours as
/// `38:2:R:G:B`, which a pane reads as other colours.
fn another_terminal(name: &str) -> bool {
    const NAMES: [&str; 10] = [
        "TERM_PROGRAM",
        "TERM_PROGRAM_VERSION",
        "TERM_SESSION_ID",
        "LC_TERMINAL",
        "LC_TERMINAL_VERSION",
        "COLORFGBG",
        "VTE_VERSION",
        "WT_SESSION",
        "WT_PROFILE_ID",
        "TERMINAL_EMULATOR",
    ];
    const PREFIXES: [&str; 6] =
        ["ITERM_", "KITTY_", "GHOSTTY_", "WEZTERM_", "ALACRITTY_", "KONSOLE_"];
    NAMES.contains(&name) || PREFIXES.iter().any(|prefix| name.starts_with(prefix))
}

fn command(launch: &Launch, environment: Option<&[(String, String)]>) -> CommandBuilder {
    let mut command = CommandBuilder::new(&launch.program);
    command.args(&launch.args);
    command.cwd(&launch.folder);
    if let Some(environment) = environment {
        command.env_clear();
        for (name, value) in environment.iter().filter(|(name, _)| !another_terminal(name)) {
            command.env(name, value);
        }
    } else {
        for (name, _) in std::env::vars_os() {
            if name.to_str().is_some_and(another_terminal) {
                command.env_remove(name);
            }
        }
    }
    // A pane is its own terminal, and says so as VS Code's does.
    command.env("TERM", "xterm-256color");
    command.env("COLORTERM", "truecolor");
    command.env("TERM_PROGRAM", "NoemaDeck");
    command.env("TERM_PROGRAM_VERSION", env!("CARGO_PKG_VERSION"));
    // A desktop app starts without a locale, and a shell without one counts
    // each byte of an accented letter or an icon as a character. A terminal
    // sets one when the profile does not.
    let locale = ["LC_ALL", "LC_CTYPE", "LANG"];
    if !locale.iter().any(|name| command.get_env(name).is_some_and(|value| !value.is_empty())) {
        command.env("LANG", "en_US.UTF-8");
    }
    command
}

/// Copies what the process writes into `output`, and ends the output when the
/// process does.
fn follow(
    mut reader: Box<dyn Read + Send>,
    mut child: Box<dyn Child + Send + Sync>,
    output: Arc<Output>,
) {
    let written = Arc::clone(&output);
    thread::spawn(move || {
        let mut chunk = [0; 8192];
        while let Ok(count @ 1..) = reader.read(&mut chunk) {
            written.push(&chunk[..count]);
        }
        // The terminal closed, so the process and all it started are gone.
        written.end();
    });
    // The process is watched as well. Its terminal can stay open after it has
    // gone, held by something it started.
    thread::spawn(move || {
        let _ = child.wait();
        // Its last output may still be on the way.
        thread::sleep(Duration::from_millis(200));
        output.end();
    });
}

fn size(rows: u16, cols: u16) -> PtySize {
    PtySize { rows: rows.max(1), cols: cols.max(1), pixel_width: 0, pixel_height: 0 }
}

#[cfg(test)]
mod tests {
    use std::process::Command;

    use super::*;

    fn shell(script: &str) -> Launch {
        let args = vec!["-c".into(), script.into()];
        Launch { folder: env::temp_dir(), program: "/bin/sh".into(), args }
    }

    /// Reads until the pane's process ends.
    fn read_all(panes: &Panes, pane: PaneId) -> Result<String> {
        let mut text = String::new();
        loop {
            let (more, ended) = panes.read(pane, Duration::from_secs(10))?;
            text += &more;
            if ended {
                return Ok(text);
            }
        }
    }

    #[test]
    fn a_pane_carries_input_and_output_and_knows_its_size() -> Result<()> {
        let panes = Panes::default();
        let script = "stty size; read line; echo \"got $line in $PWD\"";
        let pane = panes.open(&shell(script), None, 30, 100)?;

        panes.write(pane, "héllo\n".as_bytes())?;
        let text = read_all(&panes, pane)?;

        assert!(text.contains("30 100"), "{text:?}");
        let folder = env::temp_dir().canonicalize()?;
        assert!(text.contains(&format!("got héllo in {}", folder.display())), "{text:?}");
        Ok(())
    }

    #[test]
    fn resizing_reaches_the_process() -> Result<()> {
        let panes = Panes::default();
        let pane = panes.open(&shell("read line; stty size"), None, 24, 80)?;

        panes.resize(pane, 40, 120)?;
        panes.write(pane, b"\n")?;

        assert!(read_all(&panes, pane)?.contains("40 120"));
        Ok(())
    }

    #[test]
    fn closing_a_pane_stops_its_process() -> Result<()> {
        let panes = Panes::default();
        let pane = panes.open(&shell("echo pid=$$; exec sleep 600"), None, 24, 80)?;
        let text = panes.read(pane, Duration::from_secs(10))?.0;
        let pid = text.trim().strip_prefix("pid=").context("no process ID")?.to_owned();
        let alive = || Command::new("kill").args(["-0", &pid]).output();
        assert!(alive()?.status.success());

        panes.close(pane);

        assert!(panes.read(pane, Duration::ZERO).is_err());
        assert!(panes.write(pane, b"x").is_err());
        for _ in 0..100 {
            if !alive()?.status.success() {
                return Ok(());
            }
            thread::sleep(Duration::from_millis(50));
        }
        panic!("process {pid} outlived its pane");
    }

    #[test]
    fn a_pane_ends_with_its_process_even_if_the_terminal_stays_open() -> Result<()> {
        let panes = Panes::default();

        let pane = panes.open(&shell("sleep 20 & echo started"), None, 24, 80)?;

        assert!(read_all(&panes, pane)?.contains("started"));
        Ok(())
    }

    #[test]
    fn a_pane_can_run_with_another_environment() -> Result<()> {
        let panes = Panes::default();
        let environment = [("DECK".to_owned(), "set".to_owned())];
        let script = "echo $DECK-${HOME:-nohome}-$TERM";

        let pane = panes.open(&shell(script), Some(&environment), 24, 80)?;

        assert!(read_all(&panes, pane)?.contains("set-nohome-xterm-256color"));
        Ok(())
    }

    #[test]
    fn a_pane_does_not_claim_the_terminal_deck_was_started_from() {
        let environment = [
            ("TERM_PROGRAM".to_owned(), "iTerm.app".to_owned()),
            ("LC_TERMINAL".to_owned(), "iTerm2".to_owned()),
            ("ITERM_SESSION_ID".to_owned(), "w0t0p0".to_owned()),
            ("HOME".to_owned(), "/home/someone".to_owned()),
        ];
        let command = command(&shell("true"), Some(&environment));
        let value = |name| command.get_env(name).and_then(std::ffi::OsStr::to_str);
        assert_eq!(value("TERM_PROGRAM"), Some("NoemaDeck"));
        assert_eq!((value("LC_TERMINAL"), value("ITERM_SESSION_ID")), (None, None));
        assert_eq!(value("HOME"), Some("/home/someone"));
    }

    #[test]
    fn a_pane_has_a_locale_and_keeps_the_profiles_own() {
        let launch = shell("true");
        let lang = |environment: &[(String, String)]| {
            command(&launch, Some(environment)).get_env("LANG").map(std::ffi::OsStr::to_owned)
        };
        assert_eq!(lang(&[]), Some("en_US.UTF-8".into()));
        assert_eq!(lang(&[("LANG".into(), "C.UTF-8".into())]), Some("C.UTF-8".into()));
        assert_eq!(lang(&[("LC_ALL".into(), "de_DE.UTF-8".into())]), None);
    }
}
