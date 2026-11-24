//! What a pane's process has written and nobody has read yet.

use std::time::Duration;

use parking_lot::{Condvar, Mutex};

/// Output a pane holds for a slow reader before its process has to wait.
const BACKLOG: usize = 4 << 20;

#[derive(Default)]
pub(super) struct Output {
    state: Mutex<State>,
    changed: Condvar,
}

#[derive(Default)]
struct State {
    bytes: Vec<u8>,
    /// The process ended or the pane was closed.
    ended: bool,
}

impl Output {
    /// Adds what the process wrote, waiting first while the backlog is full.
    pub fn push(&self, bytes: &[u8]) {
        let mut state = self.state.lock();
        self.changed.wait_while(&mut state, |state| state.bytes.len() > BACKLOG && !state.ended);
        state.bytes.extend_from_slice(bytes);
        self.changed.notify_all();
    }

    /// Marks that no more output will come.
    pub fn end(&self) {
        self.state.lock().ended = true;
        self.changed.notify_all();
    }

    /// Waits up to `wait` for output. Answers with the text so far, and whether
    /// the output has ended with nothing left to read.
    pub fn take(&self, wait: Duration) -> (String, bool) {
        let mut state = self.state.lock();
        let idle = |state: &mut State| state.bytes.is_empty() && !state.ended;
        self.changed.wait_while_for(&mut state, idle, wait);
        let ended = state.ended;
        let text = take_text(&mut state.bytes, ended);
        self.changed.notify_all();
        (text, ended && state.bytes.is_empty())
    }
}

/// Takes the text in `bytes`, leaving a character cut off at the end for the
/// next read unless this is the `last` one.
fn take_text(bytes: &mut Vec<u8>, last: bool) -> String {
    let whole = match std::str::from_utf8(bytes) {
        Err(error) if error.error_len().is_none() && !last => error.valid_up_to(),
        _ => bytes.len(),
    };
    let text = String::from_utf8_lossy(&bytes[..whole]).into_owned();
    bytes.drain(..whole);
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_split_inside_a_character_waits_for_the_rest() {
        let mut bytes = "né".as_bytes()[..2].to_vec();
        assert_eq!(take_text(&mut bytes, false), "n");
        bytes.push("é".as_bytes()[1]);
        assert_eq!(take_text(&mut bytes, false), "é");
        let mut cut = vec![b'a', 0xC3];
        assert_eq!(take_text(&mut cut, true), "a\u{FFFD}");
    }

    #[test]
    fn output_is_taken_once_and_ends_when_nothing_is_left() {
        let output = Output::default();
        output.push(b"one ");
        output.push(b"two");

        assert_eq!(output.take(Duration::ZERO), ("one two".into(), false));
        assert_eq!(output.take(Duration::ZERO), (String::new(), false));
        output.push(b"last");
        output.end();
        assert_eq!(output.take(Duration::ZERO), ("last".into(), true));
    }
}
