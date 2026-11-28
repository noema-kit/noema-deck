//! Sessions found by what was said in them.
//!
//! Each agent's records hold what the person typed and what the agent
//! answered. Deck reads that text where the agent left it, remembers each
//! file as last read, and looks through it for a phrase. It stores nothing of
//! its own. What tools printed and what a model reasoned is left out: it is
//! most of a record, and seldom what a session is remembered by.
//!
//! | Agent | Reads |
//! | --- | --- |
//! | Claude Code | The text of `user` and `assistant` records in a session's transcript |
//! | Codex | The text of `user` and `assistant` messages in a rollout a person can resume |
//! | Antigravity | The content of `USER_INPUT` and `PLANNER_RESPONSE` steps in a conversation's transcript |

mod antigravity;
mod claude;
mod codex;

use std::{collections::BTreeMap, path::PathBuf};

use anyhow::Result;
use serde::{Deserialize, Serialize};

use crate::{
    agent::{self, Agent, Homes},
    records::Kept,
    session::SessionKey,
};

/// The fewest characters looked for. One letter is in every session.
const SHORTEST: usize = 2;
/// How many characters of the text an excerpt shows before a find and from it on.
const AROUND: (usize, usize) = (32, 120);

/// What to look for.
#[derive(Debug, Deserialize)]
pub struct Query {
    pub text: String,
}

/// A session that holds the phrase.
#[derive(Debug, PartialEq, Eq, Serialize)]
pub struct Hit {
    #[serde(flatten)]
    pub key: SessionKey,
    /// How many times the phrase occurs.
    pub count: usize,
    /// The first occurrence as it was written, between what stood before and
    /// after it, each on one line.
    pub before: String,
    pub found: String,
    pub after: String,
}

/// What one file holds of a session: every message, a line or more each.
#[derive(Default)]
struct Said {
    session: String,
    text: String,
}

impl Said {
    fn add(&mut self, message: &str) {
        let message = message.trim();
        if !message.is_empty() {
            self.text.push_str(message);
            self.text.push('\n');
        }
    }
}

/// What was said in every session, as last read.
#[derive(Default)]
pub struct Texts {
    /// Every file, in the order read.
    files: Vec<(Agent, PathBuf)>,
    read: Kept<Said>,
}

impl Texts {
    /// Reads whatever changed since last time, and finds the sessions that
    /// hold `phrase`, whatever its letters' case. The first call reads every
    /// record, which takes seconds; a phrase too short to look for finds
    /// nothing, and so only reads.
    pub fn find(&mut self, homes: &Homes, phrase: &str) -> Result<Vec<Hit>> {
        self.read(homes)?;
        let phrase = phrase.trim().to_ascii_lowercase();
        if phrase.chars().count() < SHORTEST {
            return Ok(Vec::new());
        }
        let mut hits = BTreeMap::<SessionKey, Hit>::new();
        for (agent, path) in &self.files {
            let Some(said) = self.read.get(path) else { continue };
            let mut spots = spots(&said.text, phrase.as_bytes());
            let Some(first) = spots.next() else { continue };
            let count = 1 + spots.count();
            let key = SessionKey { agent: *agent, id: said.session.clone() };
            // A session kept in two files, such as an archived copy, counts in both.
            hits.entry(key.clone())
                .and_modify(|hit| hit.count += count)
                .or_insert_with(|| excerpt(key, count, &said.text, first, phrase.len()));
        }
        Ok(hits.into_values().collect())
    }

    /// Reads the files that changed since last time.
    fn read(&mut self, homes: &Homes) -> Result<()> {
        let listed = [
            (Agent::Claude, claude::files(&homes.claude)?),
            (Agent::Codex, agent::codex::every_rollout(&homes.codex)?),
            (Agent::Antigravity, agent::antigravity::transcripts(&homes.antigravity)?),
        ];
        let mut files: Vec<_> = listed
            .into_iter()
            .flat_map(|(agent, paths)| paths.into_iter().map(move |path| (agent, path)))
            .collect();
        files.sort();
        self.read.refresh(&files, |agent, path| match agent {
            Agent::Claude => claude::read(path),
            Agent::Codex => codex::read(path),
            Agent::Antigravity => antigravity::read(path),
        })?;
        self.files = files;
        Ok(())
    }
}

/// Where `phrase`, given in lower case, occurs in `text`, one occurrence
/// after another. Only the letters A to Z match in either case: folding the
/// rest would move every letter after one that changes length.
fn spots<'a>(text: &'a str, phrase: &'a [u8]) -> impl Iterator<Item = usize> + 'a {
    let text = text.as_bytes();
    let mut from = 0;
    std::iter::from_fn(move || {
        let ahead = text.get(from..)?;
        let at =
            ahead.windows(phrase.len()).position(|window| window.eq_ignore_ascii_case(phrase))?;
        from += at + phrase.len();
        Some(from - phrase.len())
    })
}

/// The hit for a find of `len` bytes at `at`, with the text around it.
fn excerpt(key: SessionKey, count: usize, text: &str, at: usize, len: usize) -> Hit {
    let (head, tail) = (&text[..at], &text[at + len..]);
    let start = head.char_indices().rev().nth(AROUND.0 - 1).map_or(0, |(index, _)| index);
    let end = tail.char_indices().nth(AROUND.1).map_or(tail.len(), |(index, _)| index);
    // A find at the start of a message shows nothing of the one before it.
    let before = head[start..].rsplit('\n').next().unwrap_or_default();
    let cut = if start > 0 && before.len() == at - start { "…" } else { "" };
    let more = if end < tail.len() { "…" } else { "" };
    Hit {
        key,
        count,
        before: format!("{cut}{}", one_line(before).trim_start()),
        found: one_line(&text[at..at + len]),
        after: format!("{}{more}", one_line(&tail[..end]).trim_end()),
    }
}

/// `text` with each run of spaces and line ends as one space.
fn one_line(text: &str) -> String {
    let mut line = String::with_capacity(text.len());
    for word in text.split_inclusive(char::is_whitespace) {
        let bare = word.trim_end();
        line.push_str(bare);
        if bare.len() < word.len() && !line.ends_with(' ') {
            line.push(' ');
        }
    }
    line
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    fn homes(root: &std::path::Path) -> Homes {
        Homes {
            claude: root.join("claude"),
            codex: root.join("codex"),
            antigravity: root.join("antigravity-cli"),
        }
    }

    #[test]
    fn a_phrase_finds_the_sessions_of_every_agent_that_said_it() -> Result<()> {
        let root = tempfile::tempdir()?;
        let homes = homes(root.path());
        let claude = homes.claude.join("project");
        fs::create_dir_all(claude.join("one/subagents"))?;
        fs::write(
            claude.join("one.jsonl"),
            [
                r#"{"type":"user","message":{"role":"user","content":"Why does the  Journal\nreplay twice?"}}"#,
                r#"{"type":"user","isMeta":true,"message":{"role":"user","content":"a journal the person never typed"}}"#,
                r#"{"type":"user","message":{"content":[{"type":"tool_result","content":"journal in a tool's output"}]}}"#,
                r#"{"type":"assistant","message":{"content":[{"type":"thinking","thinking":"journal, thought"},{"type":"text","text":"The journal is replayed once per mount."}]}}"#,
            ]
            .join("\n"),
        )?;
        // A sub-agent's transcript is not a session of its own.
        fs::write(
            claude.join("one/subagents/two.jsonl"),
            r#"{"type":"user","message":{"content":"journal"}}"#,
        )?;
        let day = homes.codex.join("sessions/2026/11/28");
        fs::create_dir_all(&day)?;
        let id = "01a1fcb0-7894-7840-8ee2-fbb77112253b";
        fs::write(
            day.join(format!("rollout-2026-11-28T10-00-00-{id}.jsonl")),
            [
                format!(r#"{{"type":"session_meta","payload":{{"id":"{id}","source":"cli"}}}}"#),
                r#"{"type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"<environment_context>journal</environment_context>"}]}}"#.into(),
                r#"{"type":"response_item","payload":{"type":"message","role":"assistant","content":[{"type":"output_text","text":"Écrit dans le JOURNAL."}]}}"#.into(),
            ]
            .join("\n"),
        )?;
        let logs = homes.antigravity.join("brain/c1/.system_generated/logs");
        fs::create_dir_all(&logs)?;
        fs::write(
            logs.join("transcript_full.jsonl"),
            r#"{"step_index":0,"type":"USER_INPUT","status":"DONE","content":"No diary here"}"#,
        )?;

        let mut texts = Texts::default();
        let hits = texts.find(&homes, " journal ")?;
        let found: Vec<_> =
            hits.iter().map(|hit| (hit.key.agent, hit.key.id.as_str(), hit.count)).collect();
        assert_eq!(found, [(Agent::Claude, "one", 2), (Agent::Codex, id, 1)]);
        // The find keeps its own letters, on one line with what stood around it.
        let hit = &hits[0];
        assert_eq!(
            (&*hit.before, &*hit.found, &*hit.after),
            ("Why does the ", "Journal", " replay twice? The journal is replayed once per mount.")
        );
        assert_eq!((&*hits[1].before, &*hits[1].found), ("Écrit dans le ", "JOURNAL"));

        assert_eq!(texts.find(&homes, "diary")?.len(), 1);
        assert!(texts.find(&homes, "j")?.is_empty());
        assert!(texts.find(&homes, "never typed")?.is_empty());
        Ok(())
    }

    #[test]
    fn an_excerpt_is_cut_to_what_stands_around_the_find() {
        let key = SessionKey { agent: Agent::Claude, id: "one".into() };
        let text = format!("{}\nbefore the needle{}", "x".repeat(50), " y".repeat(100));
        let at = text.find("needle").unwrap_or_default();
        let hit = excerpt(key, 1, &text, at, 6);
        assert_eq!(hit.before, "before the ");
        assert!(hit.after.ends_with('…') && hit.after.chars().count() <= AROUND.1 + 1);
        // A long line before the find is cut at its start.
        let text = format!("{} needle", "é".repeat(60));
        let at = text.find("needle").unwrap_or_default();
        let key = SessionKey { agent: Agent::Claude, id: "one".into() };
        let hit = excerpt(key, 1, &text, at, 6);
        assert!(hit.before.starts_with("…é") && hit.before.chars().count() == AROUND.0 + 1);
    }
}
