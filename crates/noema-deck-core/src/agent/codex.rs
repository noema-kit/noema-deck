//! Codex adapter.
//!
//! Codex keeps one rollout per session at
//! `<home>/sessions/<year>/<month>/<day>/rollout-<time>-<id>.jsonl`, which it
//! compresses with zstd once the session is idle, and the names sessions are
//! given in `<home>/session_index.jsonl`. Neither format is documented, so
//! only the few fields read here are relied on.

use std::{
    collections::HashMap,
    env,
    fs::{self, File},
    io::{BufRead, BufReader, Read},
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use anyhow::{Context, Result, ensure};
use chrono::{SecondsFormat, Utc};
use serde::Deserialize;

use super::{Agent, Launch, append, clean_title, entries, is_uuid};
use crate::session::{Session, SessionKey};

/// Where Codex keeps the names sessions were given, by session.
const INDEX: &str = "session_index.jsonl";

/// The sources of sessions a person can resume: the terminal interface, and
/// the editor and desktop apps. Sub-agents and non-interactive runs are left
/// out, as Codex's own `resume` does.
const INTERACTIVE: [&str; 2] = ["cli", "vscode"];

/// Codex's own folder: `$CODEX_HOME`, else `~/.codex`.
pub fn home() -> Result<PathBuf> {
    match env::var_os("CODEX_HOME") {
        Some(dir) if !dir.is_empty() => Ok(dir.into()),
        _ => Ok(env::home_dir().context("No home directory")?.join(".codex")),
    }
}

/// Remembers each rollout as last read, as Claude Code's listing does.
#[derive(Default)]
pub struct Listing {
    /// Each rollout, or `None` for one that is not a session a person can resume.
    read: HashMap<PathBuf, (Stamp, Option<Rollout>)>,
}

type Stamp = (SystemTime, u64);

/// What a rollout says about its session.
#[derive(Clone)]
struct Rollout {
    id: String,
    folder: Option<PathBuf>,
    /// The first prompt typed, which names a session nobody named.
    prompt: Option<String>,
}

impl Listing {
    /// Every session a person can resume under the Codex folder `home`, in no
    /// order. A machine where Codex has never run has none.
    pub fn sessions(&mut self, home: &Path) -> Result<Vec<Session>> {
        let names = names(&home.join(INDEX))?;
        let mut found = HashMap::new();
        for rollout in rollouts(&home.join("sessions"))? {
            // Codex replaces a rollout with its compressed copy, so one listed
            // a moment ago may be gone.
            let Ok(metadata) = fs::metadata(&rollout) else { continue };
            let stamp = (metadata.modified()?, metadata.len());
            let known = self.read.remove(&rollout).filter(|(read, _)| *read == stamp);
            let read = match known {
                Some((_, read)) => read,
                None => read_rollout(&rollout),
            };
            found.insert(rollout, (stamp, read));
        }
        self.read = found;
        let sessions = self.read.values().filter_map(|(stamp, rollout)| {
            let Rollout { id, folder, prompt } = rollout.clone()?;
            // A session nobody typed in or named is empty, and Codex does not offer it.
            let title = names.get(&id).cloned().or(prompt)?;
            let since_epoch = stamp.0.duration_since(UNIX_EPOCH).ok();
            let updated = since_epoch.and_then(|age| i64::try_from(age.as_secs()).ok());
            let key = SessionKey { agent: Agent::Codex, id };
            Some(Session { key, title, folder, updated: updated.unwrap_or(0) })
        });
        // A rollout being compressed is briefly there twice.
        Ok(latest(sessions))
    }
}

/// Names a session the way Codex does, by adding its name to the index, where
/// `codex resume` also finds sessions by name. Answers with the name as stored.
pub fn rename(home: &Path, id: &str, title: &str) -> Result<String> {
    let title = clean_title(title).context("Enter a name for the session")?;
    let now = Utc::now().to_rfc3339_opts(SecondsFormat::Micros, true);
    let record = serde_json::json!({ "id": id, "thread_name": title, "updated_at": now });
    append(&home.join(INDEX), &record)?;
    Ok(title)
}

/// The command that reopens `session` in Codex's own interface.
pub fn open(session: &Session) -> Result<Launch> {
    let id = &session.key.id;
    let folder = session.folder.clone().with_context(|| format!("Session {id} has no folder"))?;
    ensure!(folder.is_dir(), "Folder {} of session {id} no longer exists", folder.display());
    Ok(Launch { folder, program: program(), args: vec!["resume".into(), id.clone()] })
}

/// The command that starts a new session in `folder`. Codex chooses the
/// session's ID itself and stores it once it has begun.
pub fn start(folder: &Path) -> Result<Launch> {
    ensure!(folder.is_dir(), "{} is not a folder", folder.display());
    Ok(Launch { folder: folder.into(), program: program(), args: Vec::new() })
}

/// The Codex executable: `$NOEMA_DECK_CODEX` when set, else `codex` wherever
/// the `PATH` finds it.
fn program() -> String {
    super::program("NOEMA_DECK_CODEX", "codex")
}

/// Keeps the most recent of several sessions found under one ID.
fn latest(sessions: impl IntoIterator<Item = Session>) -> Vec<Session> {
    let mut by_id = HashMap::<String, Session>::new();
    for session in sessions {
        match by_id.get(&session.key.id) {
            Some(known) if known.updated >= session.updated => {}
            _ => {
                by_id.insert(session.key.id.clone(), session);
            }
        }
    }
    by_id.into_values().collect()
}

/// The latest name of each session in the index. Later lines rename.
fn names(index: &Path) -> Result<HashMap<String, String>> {
    #[derive(Deserialize)]
    struct Named {
        id: String,
        thread_name: String,
    }
    let Ok(file) = File::open(index) else { return Ok(HashMap::new()) };
    let mut names = HashMap::new();
    for line in BufReader::new(file).lines() {
        let line = line.with_context(|| format!("Read {}", index.display()))?;
        if let Ok(named) = serde_json::from_str::<Named>(&line)
            && let Some(name) = clean_title(&named.thread_name)
        {
            names.insert(named.id, name);
        }
    }
    Ok(names)
}

/// Every rollout under `sessions`, which holds a folder per year, month and day.
pub(crate) fn rollouts(sessions: &Path) -> Result<Vec<PathBuf>> {
    let mut found = Vec::new();
    for year in entries(sessions)?.iter().filter(|path| path.is_dir()) {
        for month in entries(year)?.iter().filter(|path| path.is_dir()) {
            for day in entries(month)?.iter().filter(|path| path.is_dir()) {
                found.extend(entries(day)?.into_iter().filter(|path| is_rollout(path)));
            }
        }
    }
    Ok(found)
}

/// Every rollout Codex moved to `<home>/archived_sessions`, which holds no
/// folders.
pub(crate) fn archived(home: &Path) -> Result<Vec<PathBuf>> {
    Ok(entries(&home.join("archived_sessions"))?
        .into_iter()
        .filter(|path| is_rollout(path))
        .collect())
}

/// Opens a rollout to read, uncompressing one Codex has compressed.
pub(crate) fn open_rollout(path: &Path) -> Option<Box<dyn Read>> {
    let file = File::open(path).ok()?;
    if path.extension().is_some_and(|extension| extension == "zst") {
        Some(Box::new(zstd::Decoder::new(file).ok()?))
    } else {
        Some(Box::new(file))
    }
}

/// Whether `path` is a rollout, `rollout-<time>-<uuid>.jsonl`, compressed or not.
fn is_rollout(path: &Path) -> bool {
    let Some(name) = path.file_name().and_then(|name| name.to_str()) else { return false };
    let stem = name.strip_suffix(".jsonl").or_else(|| name.strip_suffix(".jsonl.zst"));
    let id = stem
        .filter(|stem| stem.starts_with("rollout-"))
        .and_then(|stem| stem.len().checked_sub(36).and_then(|start| stem.get(start..)));
    id.is_some_and(is_uuid) && path.is_file()
}

/// The first record of a rollout.
#[derive(Deserialize)]
struct First {
    #[serde(rename = "type")]
    kind: String,
    payload: Meta,
}

#[derive(Deserialize)]
struct Meta {
    id: String,
    cwd: Option<PathBuf>,
    /// A name such as `cli`, or for a sub-agent a record saying whose it is.
    source: serde_json::Value,
}

/// A record holding a message, of which only a person's prompts are read.
#[derive(Deserialize)]
struct Item {
    #[serde(rename = "type")]
    kind: String,
    payload: Message,
}

#[derive(Deserialize)]
struct Message {
    #[serde(rename = "type")]
    kind: String,
    role: Option<String>,
    #[serde(default)]
    content: Vec<Content>,
}

#[derive(Deserialize)]
struct Content {
    text: Option<String>,
}

/// Reads what Deck needs from a rollout, or `None` when it is not a session
/// a person can resume, or cannot be read.
fn read_rollout(path: &Path) -> Option<Rollout> {
    let mut reader = BufReader::new(open_rollout(path)?);
    let mut line = Vec::new();
    reader.read_until(b'\n', &mut line).ok()?;
    let first: First = serde_json::from_slice(&line).ok()?;
    let interactive =
        first.payload.source.as_str().is_some_and(|source| INTERACTIVE.contains(&source));
    if first.kind != "session_meta" || !interactive {
        return None;
    }
    let mut prompt = None;
    // Lines run to megabytes, so parse only those that can hold a prompt, and
    // stop at the first one.
    while prompt.is_none() {
        line.clear();
        if reader.read_until(b'\n', &mut line).unwrap_or(0) == 0 {
            break;
        }
        let Ok(text) = std::str::from_utf8(&line) else { continue };
        if !text.contains("\"role\":\"user\"") {
            continue;
        }
        let Ok(item) = serde_json::from_str::<Item>(text) else { continue };
        let message = item.payload;
        if item.kind != "response_item"
            || message.kind != "message"
            || message.role.as_deref() != Some("user")
        {
            continue;
        }
        prompt = message
            .content
            .into_iter()
            .filter_map(|content| content.text)
            .find_map(|text| typed(&text));
    }
    Some(Rollout { id: first.payload.id, folder: first.payload.cwd, prompt })
}

/// The first line of a prompt a person typed. Codex sends its context and
/// instructions as messages from the person too, in tags or under a heading.
fn typed(text: &str) -> Option<String> {
    let text = text.trim_start();
    if text.starts_with('<') || text.starts_with("# AGENTS.md") {
        return None;
    }
    text.lines().map(str::trim).find(|line| !line.is_empty()).map(String::from)
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use serde_json::{Value, json};

    use super::*;

    const FIRST: &str = "01a1fcb0-7894-7840-8ee2-fbb77112253b";
    const SECOND: &str = "01a2168a-b66b-7a93-9f0c-0994edc8ba9b";
    const AGENT: &str = "01a2167c-bd4a-7d50-ac32-77a78ca5647d";

    fn meta(id: &str, source: &Value) -> Value {
        json!({ "type": "session_meta", "payload": { "id": id, "cwd": "/work", "source": source } })
    }

    fn said(role: &str, text: &str) -> Value {
        json!({ "type": "response_item", "payload": {
            "type": "message", "role": role, "content": [{ "type": "input_text", "text": text }],
        } })
    }

    /// Writes a rollout for `id`, compressed if `zst`, last changed at `modified`.
    fn rollout(home: &Path, id: &str, lines: &[Value], zst: bool, modified: u64) -> Result<()> {
        let day = home.join("sessions/2026/11/26");
        fs::create_dir_all(&day)?;
        let text = lines.iter().map(|line| format!("{line}\n")).collect::<Vec<_>>().concat();
        let name = format!("rollout-2026-11-26T14-28-39-{id}.jsonl");
        let (path, bytes) = if zst {
            (day.join(format!("{name}.zst")), zstd::encode_all(text.as_bytes(), 1)?)
        } else {
            (day.join(name), text.into_bytes())
        };
        fs::write(&path, bytes)?;
        File::options()
            .write(true)
            .open(path)?
            .set_modified(SystemTime::UNIX_EPOCH + Duration::from_secs(modified))?;
        Ok(())
    }

    #[test]
    fn lists_sessions_people_can_resume_compressed_or_not() -> Result<()> {
        const EMPTY: &str = "01a20000-0000-7000-8000-000000000000";
        let home = tempfile::tempdir()?;
        let context = said("user", "<environment_context>\n  <cwd>/work</cwd>");
        let rules = said("user", "# AGENTS.md instructions for /work\n\nBe brief.");
        let lines = [
            meta(FIRST, &json!("cli")),
            said("developer", "You are Codex"),
            context,
            rules,
            said("user", "\n  Count the tokens\nfor a month"),
        ];
        rollout(home.path(), FIRST, &lines, true, 100)?;
        let lines = [meta(SECOND, &json!("vscode")), said("user", "Plan the week")];
        rollout(home.path(), SECOND, &lines, false, 200)?;
        rollout(home.path(), EMPTY, &[meta(EMPTY, &json!("cli"))], false, 250)?;
        let guardian = json!({ "subagent": { "other": "guardian" } });
        rollout(home.path(), AGENT, &[meta(AGENT, &guardian), said("user", "Review")], false, 300)?;
        fs::write(home.path().join("sessions/2026/11/26/notes.jsonl"), "")?;

        let mut sessions = Listing::default().sessions(home.path())?;
        sessions.sort_by_key(|session| session.updated);

        let seen: Vec<_> = sessions
            .iter()
            .map(|session| (session.key.id.as_str(), session.title.as_str(), session.updated))
            .collect();
        assert_eq!(seen, [(FIRST, "Count the tokens", 100), (SECOND, "Plan the week", 200)]);
        assert_eq!(sessions[0].key.agent, Agent::Codex);
        assert_eq!(sessions[0].folder.as_deref(), Some(Path::new("/work")));
        Ok(())
    }

    #[test]
    fn a_name_in_the_index_titles_a_session_and_renaming_adds_one() -> Result<()> {
        let home = tempfile::tempdir()?;
        rollout(
            home.path(),
            FIRST,
            &[meta(FIRST, &json!("cli")), said("user", "Prompt")],
            false,
            100,
        )?;
        let index = home.path().join(INDEX);
        let named =
            json!({ "id": FIRST, "thread_name": "Named", "updated_at": "2026-11-26T06:13:23Z" });
        fs::write(&index, format!("{named}"))?;
        let mut listing = Listing::default();
        assert_eq!(listing.sessions(home.path())?[0].title, "Named");

        assert_eq!(rename(home.path(), FIRST, "  Deck\n rename ")?, "Deck rename");

        let sessions = listing.sessions(home.path())?;
        assert_eq!((sessions[0].title.as_str(), sessions[0].updated), ("Deck rename", 100));
        let text = fs::read_to_string(&index)?;
        assert!(text.ends_with('\n') && text.lines().count() == 2);
        let last: Value = serde_json::from_str(text.lines().last().context("line")?)?;
        assert!(last["updated_at"].as_str().is_some_and(|time| time.ends_with('Z')));
        assert!(rename(home.path(), FIRST, " ").is_err());
        Ok(())
    }

    #[test]
    fn opening_resumes_and_starting_lets_codex_choose_the_id() -> Result<()> {
        let folder = tempfile::tempdir()?;
        let key = SessionKey { agent: Agent::Codex, id: FIRST.into() };
        let session =
            Session { key, title: "Any".into(), folder: Some(folder.path().into()), updated: 0 };

        let launch = open(&session)?;

        assert_eq!(
            (launch.program.as_str(), launch.args.as_slice()),
            ("codex", ["resume", FIRST].map(String::from).as_slice())
        );
        assert_eq!(launch.folder, folder.path());
        assert!(start(folder.path())?.args.is_empty());
        assert!(start(&folder.path().join("gone")).is_err());
        Ok(())
    }
}
