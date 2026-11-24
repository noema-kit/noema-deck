//! The environment a pane's process runs in.

use std::{
    env,
    io::Read,
    process::{Command, Stdio},
    sync::{OnceLock, mpsc},
    thread,
    time::Duration,
};

/// Marks where the shell's environment starts and ends in its output, after
/// whatever its profile prints.
const MARK: &str = "\0noema-deck-environment\0";

/// The environment of the user's login shell, read once. A desktop app starts
/// without the `PATH` a terminal has, and the shell's profile is where the
/// user set it. `None` when the shell would not say.
pub(crate) fn login() -> Option<&'static [(String, String)]> {
    static ENVIRONMENT: OnceLock<Option<Vec<(String, String)>>> = OnceLock::new();
    ENVIRONMENT.get_or_init(ask_shell).as_deref()
}

fn ask_shell() -> Option<Vec<(String, String)>> {
    let shell = env::var("SHELL").ok().filter(|shell| !shell.is_empty())?;
    let mark = r"printf '\0noema-deck-environment\0'";
    let mut child = Command::new(shell)
        .args(["-l", "-i", "-c", &format!("{mark}; command env -0; {mark}")])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let mut stdout = child.stdout.take()?;
    let (found, listed) = mpsc::channel();
    // What the profile started may keep the pipe open, so reading stops at the
    // closing mark and does not wait for the pipe to close.
    thread::spawn(move || {
        let (mut bytes, mut chunk) = (Vec::new(), [0; 8192]);
        while let Ok(count @ 1..) = stdout.read(&mut chunk) {
            bytes.extend_from_slice(&chunk[..count]);
            let text = String::from_utf8_lossy(&bytes);
            if text.matches(MARK).count() == 2 {
                let _ = found.send(parse(&text));
                return;
            }
        }
    });
    let environment = listed.recv_timeout(Duration::from_secs(5)).ok().flatten();
    let _ = child.kill();
    let _ = child.wait();
    environment
}

/// The variables between the two marks, without those that describe the shell
/// that listed them.
fn parse(text: &str) -> Option<Vec<(String, String)>> {
    let listed = text.split(MARK).nth(1)?;
    let variables: Vec<_> = listed
        .split('\0')
        .filter_map(|variable| variable.split_once('='))
        .filter(|(name, _)| !["PWD", "OLDPWD", "SHLVL", "_"].contains(name))
        .map(|(name, value)| (name.to_owned(), value.to_owned()))
        .collect();
    (!variables.is_empty()).then_some(variables)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_environment_is_read_between_its_marks() {
        let listed = ["noise", "PATH=/a:/b\0PWD=/here\0EMPTY=\0X=a=b", ""].join(MARK);

        let variables = parse(&listed);

        let pair = |name: &str, value: &str| (name.to_owned(), value.to_owned());
        let expected = vec![pair("PATH", "/a:/b"), pair("EMPTY", ""), pair("X", "a=b")];
        assert_eq!(variables, Some(expected));
        assert_eq!(parse("no marks at all"), None);
    }
}
