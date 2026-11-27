//! The tokens the agents used, read from the records each keeps of its
//! sessions.
//!
//! Every agent writes down how many tokens each reply of a model took. Deck
//! reads those records where the agent left them, remembers each file as last
//! read, and adds the replies up by day, model and session. It stores nothing
//! of its own. [`limits`] says how much of each agent's plan is used.
//!
//! | Agent | Reads |
//! | --- | --- |
//! | Claude Code | `usage` of each reply in the transcripts, sub-agents' included |
//! | Codex | `token_count` events in the rollouts, archived ones included |
//! | Antigravity | Token counts of each `PLANNER_RESPONSE` in `brain/<id>/…/transcript_full.jsonl` |

mod antigravity;
mod claude;
mod codex;
pub mod limits;

pub use self::limits::{Asker, Limit, LimitsRequest, Plan};

use std::{
    collections::{BTreeMap, HashMap},
    fs,
    io::{BufRead, BufReader, Read},
    num::NonZero,
    ops::AddAssign,
    panic,
    path::{Path, PathBuf},
    sync::atomic::{AtomicUsize, Ordering},
    thread,
    time::SystemTime,
};

use anyhow::Result;
use chrono::{DateTime, Local, TimeZone};
use serde::Serialize;

use crate::{
    agent::{Agent, Homes, entries},
    session::SessionKey,
};

/// Counts of tokens, and of the replies that took them. `input` is what the
/// model read afresh; `cached` it read from the prompt cache, and `written`
/// went into that cache.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
pub struct Tokens {
    pub input: u64,
    pub cached: u64,
    pub written: u64,
    pub output: u64,
    pub replies: u64,
}

impl Tokens {
    /// Every token, of whatever kind.
    #[must_use]
    pub fn total(&self) -> u64 {
        self.input + self.cached + self.written + self.output
    }

    fn is_empty(&self) -> bool {
        self.total() == 0
    }
}

impl AddAssign for Tokens {
    fn add_assign(&mut self, other: Self) {
        self.input += other.input;
        self.cached += other.cached;
        self.written += other.written;
        self.output += other.output;
        self.replies += other.replies;
    }
}

/// What the interface draws: every reply, added up two ways.
#[derive(Debug, Default, Serialize)]
pub struct Report {
    /// Tokens by day, agent and model, oldest first.
    pub days: Vec<DayUse>,
    /// Tokens by session and day.
    pub sessions: Vec<SessionUse>,
}

#[derive(Debug, Serialize)]
pub struct DayUse {
    /// The local date, as `2026-11-28`.
    pub day: String,
    pub agent: Agent,
    pub model: String,
    #[serde(flatten)]
    pub tokens: Tokens,
}

#[derive(Debug, Serialize)]
pub struct SessionUse {
    pub day: String,
    #[serde(flatten)]
    pub key: SessionKey,
    #[serde(flatten)]
    pub tokens: Tokens,
}

/// One window of Codex's allowance, as a record gave it.
#[derive(Debug)]
struct Window {
    /// The window's length.
    minutes: u64,
    /// The share used, in percent.
    used: f64,
    /// When the window starts again, in Unix seconds.
    resets: i64,
    /// When Codex recorded this, in Unix seconds.
    seen: i64,
}

/// One reply of a model as an agent recorded it.
#[derive(Debug)]
struct Reply {
    /// Names the reply wherever the agent recorded it, so it counts once.
    key: String,
    /// Of two records of one reply, the one ranked higher counts, or the
    /// first read when they rank the same.
    rank: u64,
    session: String,
    model: String,
    /// Unix seconds.
    at: i64,
    tokens: Tokens,
}

/// What one file says.
#[derive(Default)]
struct Records {
    replies: Vec<Reply>,
    windows: Vec<Window>,
}

/// A file's modification time and length. A change to either means it has
/// to be read again.
type Stamp = (SystemTime, u64);

/// Every agent's records, as last read.
#[derive(Default)]
pub struct Ledger {
    /// Every file, in the order read.
    files: Vec<(Agent, PathBuf)>,
    read: HashMap<PathBuf, (Stamp, Records)>,
}

impl Ledger {
    /// Reads whatever changed since last time and adds up every reply.
    pub fn report(&mut self, homes: &Homes) -> Result<Report> {
        self.read(homes)?;
        let mut kept: HashMap<(Agent, &str), &Reply> = HashMap::new();
        for (agent, records) in self.records() {
            for reply in &records.replies {
                kept.entry((agent, &reply.key))
                    .and_modify(|known| {
                        if reply.rank > known.rank {
                            *known = reply;
                        }
                    })
                    .or_insert(reply);
            }
        }
        Ok(add_up(kept.into_iter().map(|((agent, _), reply)| (agent, reply))))
    }

    /// Codex's plan limits, as its rollouts last recorded them.
    pub fn codex_plan(&mut self, homes: &Homes) -> Result<Plan> {
        self.read(homes)?;
        let windows = self.records().filter(|(agent, _)| *agent == Agent::Codex);
        Ok(limits::codex_plan(windows.flat_map(|(_, records)| &records.windows)))
    }

    /// Each file's records, in the order read.
    fn records(&self) -> impl Iterator<Item = (Agent, &Records)> {
        self.files.iter().filter_map(|(agent, path)| Some((*agent, &self.read.get(path)?.1)))
    }

    /// Reads the files that changed since last time.
    fn read(&mut self, homes: &Homes) -> Result<()> {
        let listed = [
            (Agent::Claude, claude::files(&homes.claude)?),
            (Agent::Codex, codex::files(&homes.codex)?),
            (Agent::Antigravity, antigravity::files(&homes.antigravity)?),
        ];
        let mut files: Vec<_> = listed
            .into_iter()
            .flat_map(|(agent, paths)| paths.into_iter().map(move |path| (agent, path)))
            .collect();
        // Read in a set order, so the same reply recorded twice always counts
        // for the same session.
        files.sort();

        let mut found = HashMap::new();
        let mut unread = Vec::new();
        for (agent, path) in &files {
            let Ok(metadata) = fs::metadata(path) else { continue };
            let stamp = (metadata.modified()?, metadata.len());
            match self.read.remove(path).filter(|(read, _)| *read == stamp) {
                Some(known) => _ = found.insert(path.clone(), known),
                None => unread.push((*agent, path.as_path(), stamp)),
            }
        }
        for ((_, path, stamp), read) in unread.iter().zip(read_all(&unread)) {
            found.insert(path.to_path_buf(), (*stamp, read));
        }
        self.read = found;
        self.files = files;
        Ok(())
    }
}

/// Reads each file in `unread`, in order, on as many threads as there are
/// cores. Codex compresses old rollouts, and reading every one takes seconds
/// on one thread.
fn read_all(unread: &[(Agent, &Path, Stamp)]) -> Vec<Records> {
    let next = AtomicUsize::new(0);
    let workers = thread::available_parallelism().map_or(1, NonZero::get).min(unread.len());
    let mut read: Vec<(usize, Records)> = thread::scope(|scope| {
        let workers: Vec<_> = (0..workers)
            .map(|_| {
                scope.spawn(|| {
                    let mut read = Vec::new();
                    loop {
                        let at = next.fetch_add(1, Ordering::Relaxed);
                        let Some((agent, path, _)) = unread.get(at) else { break };
                        let records = match agent {
                            Agent::Claude => claude::read(path),
                            Agent::Codex => codex::read(path),
                            Agent::Antigravity => antigravity::read(path),
                        };
                        read.push((at, records));
                    }
                    read
                })
            })
            .collect();
        workers
            .into_iter()
            .flat_map(|worker| worker.join().unwrap_or_else(|panic| panic::resume_unwind(panic)))
            .collect()
    });
    read.sort_by_key(|(at, _)| *at);
    read.into_iter().map(|(_, records)| records).collect()
}

/// Adds up `replies` by day.
fn add_up<'a>(replies: impl Iterator<Item = (Agent, &'a Reply)>) -> Report {
    let mut days = BTreeMap::<(String, Agent, &str), Tokens>::new();
    let mut sessions = BTreeMap::<(String, Agent, &str), Tokens>::new();
    for (agent, reply) in replies {
        let day = day(reply.at);
        *days.entry((day.clone(), agent, &reply.model)).or_default() += reply.tokens;
        *sessions.entry((day, agent, &reply.session)).or_default() += reply.tokens;
    }
    let days = days
        .into_iter()
        .map(|((day, agent, model), tokens)| DayUse { day, agent, model: model.into(), tokens })
        .collect();
    let sessions = sessions
        .into_iter()
        .map(|((day, agent, id), tokens)| SessionUse {
            day,
            key: SessionKey { agent, id: id.into() },
            tokens,
        })
        .collect();
    Report { days, sessions }
}

/// The local date of a moment in Unix seconds.
fn day(at: i64) -> String {
    Local
        .timestamp_opt(at, 0)
        .single()
        .map_or_else(String::new, |at| at.format("%Y-%m-%d").to_string())
}

/// A moment written as RFC 3339, in Unix seconds.
fn seconds(at: &str) -> Option<i64> {
    DateTime::parse_from_rfc3339(at).ok().map(|at| at.timestamp())
}

/// Calls `each` with every line of `source` whose first `head` bytes hold one
/// of `marks`. Records run to megabytes and most hold no tokens, so a line is
/// searched before it is parsed.
fn lines(source: impl Read, head: usize, marks: &[&str], mut each: impl FnMut(&str)) {
    let mut reader = BufReader::with_capacity(1 << 16, source);
    let mut line = Vec::new();
    loop {
        line.clear();
        match reader.read_until(b'\n', &mut line) {
            Ok(0) | Err(_) => break,
            Ok(_) => {}
        }
        let head = &line[..line.len().min(head)];
        if !marks.iter().any(|mark| contains(head, mark.as_bytes())) {
            continue;
        }
        if let Ok(text) = std::str::from_utf8(&line) {
            each(text);
        }
    }
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    haystack.windows(needle.len()).any(|window| window == needle)
}

/// Every file under `dir` whose name ends with `suffix`, at most `depth`
/// folders down.
fn walk(dir: &Path, suffix: &str, depth: usize, found: &mut Vec<PathBuf>) -> Result<()> {
    for path in entries(dir)? {
        if path.is_dir() {
            if depth > 0 {
                walk(&path, suffix, depth - 1, found)?;
            }
        } else if path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.ends_with(suffix))
        {
            found.push(path);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reply(key: &str, session: &str, output: u64) -> Reply {
        Reply {
            key: key.into(),
            rank: 0,
            session: session.into(),
            model: "model".into(),
            at: Local.with_ymd_and_hms(2026, 11, 27, 12, 0, 0).unwrap().timestamp(),
            tokens: Tokens { output, replies: 1, ..Tokens::default() },
        }
    }

    #[test]
    fn adds_replies_up_by_day_model_and_session() {
        let (first, second) = (reply("a", "one", 5), reply("b", "two", 7));
        let report = add_up([(Agent::Codex, &first), (Agent::Codex, &second)].into_iter());
        assert_eq!(report.days.len(), 1);
        assert_eq!(report.days[0].day, "2026-11-27");
        assert_eq!(report.days[0].tokens, Tokens { output: 12, replies: 2, ..Tokens::default() });
        let sessions: Vec<_> =
            report.sessions.iter().map(|used| (used.key.id.as_str(), used.tokens.output)).collect();
        assert_eq!(sessions, [("one", 5), ("two", 7)]);
    }
}
