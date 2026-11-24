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
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
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
    writer: Box<dyn Write + Send>,
    killer: Box<dyn ChildKiller + Send + Sync>,
    output: Arc<Output>,
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
            writer: pair.master.take_writer().map_err(|error| anyhow!(error))?,
            killer: child.clone_killer(),
            output: Arc::clone(&output),
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
        self.with(pane, |pane| Ok(pane.writer.write_all(data)?))
    }

    pub fn resize(&self, pane: PaneId, rows: u16, cols: u16) -> Result<()> {
        self.with(pane, |pane| pane.master.resize(size(rows, cols)).map_err(|error| anyhow!(error)))
    }

    /// Stops the pane's process and forgets the pane.
    pub fn close(&self, pane: PaneId) {
        self.open.lock().remove(&pane);
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

fn command(launch: &Launch, environment: Option<&[(String, String)]>) -> CommandBuilder {
    let mut command = CommandBuilder::new(&launch.program);
    command.args(&launch.args);
    command.cwd(&launch.folder);
    if let Some(environment) = environment {
        command.env_clear();
        for (name, value) in environment {
            command.env(name, value);
        }
    }
    command.env("TERM", "xterm-256color");
    command.env("COLORTERM", "truecolor");
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
}
