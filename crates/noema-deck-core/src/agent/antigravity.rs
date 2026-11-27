//! Antigravity adapter.
//!
//! The Antigravity CLI, `agy`, keeps a summary of each conversation in
//! `<home>/conversation_summaries.db`, the title a conversation was given in
//! `<home>/annotations/<id>.pbtxt`, and the prompts typed in
//! `<home>/history.jsonl`. None of these is documented, so only the few fields
//! read here are relied on.

use std::{
    collections::HashMap,
    env, fs,
    path::{Path, PathBuf},
    time::{Duration, UNIX_EPOCH},
};

use anyhow::{Context, Result, ensure};
use chrono::DateTime;
use rusqlite::{Connection, OpenFlags};
use serde::Deserialize;

use super::{Agent, Launch, clean_title, entries, is_uuid};
use crate::session::{Session, SessionKey};

/// The conversation summaries `agy` keeps.
const SUMMARIES: &str = "conversation_summaries.db";

/// The Antigravity CLI's own folder: `$NOEMA_DECK_ANTIGRAVITY_HOME`, else
/// `~/.gemini/antigravity-cli`. `agy` itself always uses the latter.
pub fn home() -> Result<PathBuf> {
    match env::var_os("NOEMA_DECK_ANTIGRAVITY_HOME") {
        Some(dir) if !dir.is_empty() => Ok(dir.into()),
        _ => Ok(env::home_dir().context("No home directory")?.join(".gemini/antigravity-cli")),
    }
}

/// Every conversation under the folder `home` that `agy` offers to resume, in
/// no order. A machine where `agy` has never run has none.
pub fn sessions(home: &Path) -> Result<Vec<Session>> {
    let summaries = home.join(SUMMARIES);
    if !summaries.is_file() {
        return Ok(Vec::new());
    }
    let titles = titles(&home.join("annotations"))?;
    let prompts = prompts(&home.join("history.jsonl"));
    let mut sessions = Vec::new();
    for summary in read_summaries(&summaries)? {
        let Summary { id, title, folder, modified } = summary;
        // A conversation deleted from `agy` leaves its summary for a while.
        let Ok(metadata) = fs::metadata(home.join(format!("conversations/{id}.db"))) else {
            continue;
        };
        let title = titles.get(&id).cloned().or(title).or_else(|| prompts.get(&id).cloned());
        let Some(title) = title else { continue };
        let written = metadata.modified()?.duration_since(UNIX_EPOCH).unwrap_or(Duration::ZERO);
        let written = i64::try_from(written.as_secs()).unwrap_or(0);
        let key = SessionKey { agent: Agent::Antigravity, id };
        sessions.push(Session { key, title, folder, updated: written.max(modified) });
    }
    Ok(sessions)
}

/// Names a conversation the way `agy` does, in its annotations, which `agy`
/// shows and copies into its summaries. Answers with the name as stored.
pub fn rename(home: &Path, id: &str, title: &str) -> Result<String> {
    // Text format would need control characters escaped, and no name needs them.
    let title: String = title.chars().map(|c| if c.is_control() { ' ' } else { c }).collect();
    let title = clean_title(&title).context("Enter a name for the session")?;
    ensure!(is_uuid(id), "Session {id} is not an Antigravity conversation");
    let dir = home.join("annotations");
    fs::create_dir_all(&dir).with_context(|| format!("Create {}", dir.display()))?;
    let file = dir.join(format!("{id}.pbtxt"));
    let text = fs::read_to_string(&file).unwrap_or_default();
    fs::write(&file, with_title(&text, &title))
        .with_context(|| format!("Write {}", file.display()))?;
    Ok(title)
}

/// The command that reopens `session` in `agy`'s own interface.
pub fn open(session: &Session) -> Result<Launch> {
    let id = &session.key.id;
    let folder = session.folder.clone().with_context(|| format!("Session {id} has no folder"))?;
    ensure!(folder.is_dir(), "Folder {} of session {id} no longer exists", folder.display());
    Ok(Launch { folder, program: program(), args: vec!["--conversation".into(), id.clone()] })
}

/// The command that starts a new conversation in `folder`. `agy` chooses the
/// conversation's ID itself and lists it once it has a first step.
pub fn start(folder: &Path) -> Result<Launch> {
    ensure!(folder.is_dir(), "{} is not a folder", folder.display());
    Ok(Launch { folder: folder.into(), program: program(), args: Vec::new() })
}

/// Gives `launch` the colour scheme matching the one chosen in `agy`'s
/// settings for a light or a dark terminal, when the chosen one is of the
/// other kind, for this run only.
#[must_use]
pub fn for_pane(mut launch: Launch, light: bool) -> Launch {
    let chosen = home().ok().and_then(|home| color_scheme(&home.join("settings.json")));
    if let Some(scheme) = chosen.and_then(|chosen| counterpart(&chosen, light)) {
        let settings = serde_json::json!({ "colorScheme": scheme });
        launch.args.extend(["--settings".into(), settings.to_string()]);
    }
    launch
}

/// The executable: `$NOEMA_DECK_ANTIGRAVITY` when set, else `agy` wherever
/// the `PATH` finds it.
pub(crate) fn program() -> String {
    super::program("NOEMA_DECK_ANTIGRAVITY", "agy")
}

/// What a summary says about its conversation.
struct Summary {
    id: String,
    title: Option<String>,
    folder: Option<PathBuf>,
    /// When it last changed, in seconds since the epoch, or 0 if unknown.
    modified: i64,
}

/// The summaries of conversations a person had: those with at least one step,
/// and not a sub-agent's.
fn read_summaries(file: &Path) -> Result<Vec<Summary>> {
    let unread = || format!("Read {}", file.display());
    let flags = OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX;
    let connection = Connection::open_with_flags(file, flags).with_context(unread)?;
    connection.busy_timeout(Duration::from_millis(500))?;
    let mut statement = connection
        .prepare(
            "SELECT conversation_id, title, workspace_uris, CAST(last_modified_time AS TEXT)
             FROM conversation_summaries
             WHERE step_count > 0 AND parent_conversation_id = '' AND nesting_depth = 0",
        )
        .with_context(unread)?;
    let rows = statement.query_map([], |row| {
        Ok((row.get::<_, String>(0)?, row.get(1)?, row.get::<_, String>(2)?, row.get(3)?))
    })?;
    let mut summaries = Vec::new();
    for row in rows {
        let (id, title, folders, modified): (String, String, String, String) =
            row.with_context(unread)?;
        let folder = serde_json::from_str::<Vec<String>>(&folders)
            .ok()
            .and_then(|uris| uris.iter().find_map(|uri| local_path(uri)));
        let modified = DateTime::parse_from_str(&modified, "%Y-%m-%d %H:%M:%S%.f%:z")
            .map_or(0, |time| time.timestamp().max(0));
        summaries.push(Summary { id, title: clean_title(&title), folder, modified });
    }
    Ok(summaries)
}

/// The path a `file://` URI names.
fn local_path(uri: &str) -> Option<PathBuf> {
    let path = uri.strip_prefix("file://")?;
    let bytes = path.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        let escaped = (bytes[index] == b'%')
            .then(|| path.get(index + 1..index + 3))
            .flatten()
            .and_then(|hex| u8::from_str_radix(hex, 16).ok());
        if let Some(byte) = escaped {
            decoded.push(byte);
            index += 3;
        } else {
            decoded.push(bytes[index]);
            index += 1;
        }
    }
    String::from_utf8(decoded).ok().map(PathBuf::from)
}

/// The title each annotated conversation was given, by conversation.
fn titles(annotations: &Path) -> Result<HashMap<String, String>> {
    let mut titles = HashMap::new();
    for file in entries(annotations)? {
        let Some(id) = file.file_name().and_then(|name| name.to_str()?.strip_suffix(".pbtxt"))
        else {
            continue;
        };
        let Ok(text) = fs::read_to_string(&file) else { continue };
        if let Some(title) = title_field(&text).and_then(|(_, title)| clean_title(&title)) {
            titles.insert(id.to_owned(), title);
        }
    }
    Ok(titles)
}

/// The first prompt typed in each conversation, by conversation. Commands
/// such as `/model` are not prompts.
fn prompts(history: &Path) -> HashMap<String, String> {
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Typed {
        display: String,
        conversation_id: Option<String>,
        #[serde(rename = "type")]
        kind: Option<String>,
    }
    let mut prompts = HashMap::new();
    let text = fs::read_to_string(history).unwrap_or_default();
    for typed in text.lines().filter_map(|line| serde_json::from_str::<Typed>(line).ok()) {
        let (Some(id), None) = (typed.conversation_id, typed.kind) else { continue };
        if let Some(line) = typed.display.lines().find_map(clean_title) {
            prompts.entry(id).or_insert(line);
        }
    }
    prompts
}

/// The span and value of the top-level `title` field of a text-format
/// protocol buffer message.
fn title_field(text: &str) -> Option<(std::ops::Range<usize>, String)> {
    let bytes = text.as_bytes();
    let mut depth = 0_usize;
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'"' | b'\'' => index = quoted(bytes, index)?.0,
            b'{' | b'<' => {
                depth += 1;
                index += 1;
            }
            b'}' | b'>' => {
                depth = depth.saturating_sub(1);
                index += 1;
            }
            byte if byte.is_ascii_alphabetic() || byte == b'_' => {
                let start = index;
                while index < bytes.len()
                    && (bytes[index].is_ascii_alphanumeric() || bytes[index] == b'_')
                {
                    index += 1;
                }
                if depth > 0 || &text[start..index] != "title" {
                    continue;
                }
                let mut value = index;
                while bytes.get(value).is_some_and(u8::is_ascii_whitespace) {
                    value += 1;
                }
                if bytes.get(value) != Some(&b':') {
                    continue;
                }
                value += 1;
                while bytes.get(value).is_some_and(u8::is_ascii_whitespace) {
                    value += 1;
                }
                if matches!(bytes.get(value), Some(b'"' | b'\'')) {
                    let (end, title) = quoted(bytes, value)?;
                    return Some((start..end, title));
                }
            }
            _ => index += 1,
        }
    }
    None
}

/// Reads the quoted string starting at `start`, answering with the index just
/// past it and its value.
fn quoted(bytes: &[u8], start: usize) -> Option<(usize, String)> {
    let quote = bytes[start];
    let mut value = Vec::new();
    let mut index = start + 1;
    loop {
        let byte = *bytes.get(index)?;
        index += 1;
        if byte == quote {
            return Some((index, String::from_utf8_lossy(&value).into_owned()));
        }
        if byte != b'\\' {
            value.push(byte);
            continue;
        }
        let escape = *bytes.get(index)?;
        index += 1;
        let digits = |index: usize, radix: u32, most: usize| {
            let run = bytes[index..]
                .iter()
                .take(most)
                .take_while(|byte| char::from(**byte).is_digit(radix))
                .count();
            let text = std::str::from_utf8(&bytes[index..index + run]).ok()?;
            Some((u8::from_str_radix(text, radix).ok()?, run))
        };
        match escape {
            b'n' => value.push(b'\n'),
            b't' => value.push(b'\t'),
            b'r' => value.push(b'\r'),
            b'x' => {
                let (byte, run) = digits(index, 16, 2)?;
                value.push(byte);
                index += run;
            }
            b'0'..=b'7' => {
                let (byte, run) = digits(index - 1, 8, 3)?;
                value.push(byte);
                index += run - 1;
            }
            other => value.push(other),
        }
    }
}

/// `text` with its top-level `title` field set to `title`.
fn with_title(text: &str, title: &str) -> String {
    let mut field = String::from("title:\"");
    for character in title.chars() {
        match character {
            '"' => field.push_str("\\\""),
            '\\' => field.push_str("\\\\"),
            other => field.push(other),
        }
    }
    field.push('"');
    match title_field(text) {
        Some((span, _)) => format!("{}{field}{}", &text[..span.start], &text[span.end..]),
        None if text.trim().is_empty() => field,
        None => format!("{field}\n{text}"),
    }
}

/// The colour scheme an `agy` settings file names, if it can be read and has one.
fn color_scheme(settings: &Path) -> Option<String> {
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Settings {
        color_scheme: String,
    }
    let text = fs::read_to_string(settings).ok()?;
    Some(serde_json::from_str::<Settings>(&text).ok()?.color_scheme)
}

/// The light or dark counterpart of the scheme `chosen`, named by its last
/// word, when `chosen` is of the other kind.
fn counterpart(chosen: &str, light: bool) -> Option<String> {
    let (from, to) = if light { ("dark", "light") } else { ("light", "dark") };
    let base = chosen.strip_suffix(from)?;
    (base.is_empty() || base.ends_with(' ')).then(|| format!("{base}{to}"))
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    const FIRST: &str = "92f0283a-29bf-4aa6-b88a-9d60c0e6d99e";
    const SECOND: &str = "2240b10a-706d-4df9-a89f-26cc426d9a52";
    const EMPTY: &str = "6b4c3a20-0000-4000-8000-000000000000";
    const AGENT: &str = "7c5d4b30-0000-4000-8000-000000000000";

    /// A summaries table with the columns Deck reads, and the rest as `agy` has them.
    fn summaries(home: &Path, rows: &[(&str, &str, &str, i64, &str, &str)]) -> Result<()> {
        let connection = Connection::open(home.join(SUMMARIES))?;
        connection.execute_batch(
            "CREATE TABLE conversation_summaries (conversation_id text, title text NOT NULL DEFAULT '',
             step_count integer NOT NULL DEFAULT 0, last_modified_time datetime NOT NULL,
             workspace_uris text NOT NULL, parent_conversation_id text NOT NULL DEFAULT '',
             nesting_depth integer NOT NULL DEFAULT 0, PRIMARY KEY (conversation_id))",
        )?;
        for (id, title, folder, steps, time, parent) in rows {
            connection.execute(
                "INSERT INTO conversation_summaries VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                rusqlite::params![
                    id,
                    title,
                    steps,
                    time,
                    json!([format!("file://{folder}")]).to_string(),
                    parent,
                    i64::from(!parent.is_empty()),
                ],
            )?;
            fs::create_dir_all(home.join("conversations"))?;
            fs::write(home.join(format!("conversations/{id}.db")), "")?;
        }
        Ok(())
    }

    #[test]
    fn lists_conversations_people_had_by_their_titles() -> Result<()> {
        let home = tempfile::tempdir()?;
        let late = "2100-01-01 00:00:00.5+00:00";
        summaries(
            home.path(),
            &[
                (FIRST, "Quick Status Check", "/Work%20Space", 1, late, ""),
                (SECOND, "", "/work", 2, "0001-01-01 00:00:00+00:00", ""),
                (EMPTY, "", "/work", 0, late, ""),
                (AGENT, "Helper", "/work", 3, late, FIRST),
            ],
        )?;
        let history = [
            json!({ "display": "/model", "timestamp": 1, "workspace": "/work", "type": "slash_command", "conversationId": SECOND }),
            json!({ "display": "\n  Check the account\nplease", "timestamp": 2, "workspace": "/work", "conversationId": SECOND }),
            json!({ "display": "Later", "timestamp": 3, "workspace": "/work", "conversationId": SECOND }),
        ];
        let lines: Vec<_> = history.iter().map(ToString::to_string).collect();
        fs::write(home.path().join("history.jsonl"), lines.join("\n"))?;

        let mut sessions = sessions(home.path())?;
        sessions.sort_by_key(|session| std::cmp::Reverse(session.updated));

        let seen: Vec<_> = sessions
            .iter()
            .map(|session| (session.key.id.as_str(), session.title.as_str()))
            .collect();
        assert_eq!(seen, [(FIRST, "Quick Status Check"), (SECOND, "Check the account")]);
        assert_eq!(sessions[0].key.agent, Agent::Antigravity);
        assert_eq!(sessions[0].folder.as_deref(), Some(Path::new("/Work Space")));
        assert_eq!(sessions[0].updated, 4_102_444_800);
        assert!(sessions[1].updated > 0);
        Ok(())
    }

    #[test]
    fn renaming_annotates_the_conversation() -> Result<()> {
        let home = tempfile::tempdir()?;
        summaries(
            home.path(),
            &[(FIRST, "Generated", "/work", 1, "2026-11-27 13:43:08+00:00", "")],
        )?;
        let annotation = home.path().join(format!("annotations/{FIRST}.pbtxt"));
        assert_eq!(rename(home.path(), FIRST, "  Deck\n \"rename\"\u{7} ")?, "Deck \"rename\"");
        assert_eq!(fs::read_to_string(&annotation)?, r#"title:"Deck \"rename\"""#);
        assert_eq!(sessions(home.path())?[0].title, "Deck \"rename\"");

        fs::write(
            &annotation,
            "last_user_view_time:  {seconds:  5}\ntitle:  \"Old \\303\\251\"  starred: true",
        )?;
        assert_eq!(sessions(home.path())?[0].title, "Old é");
        rename(home.path(), FIRST, "New")?;
        assert_eq!(
            fs::read_to_string(&annotation)?,
            "last_user_view_time:  {seconds:  5}\ntitle:\"New\"  starred: true"
        );
        assert!(rename(home.path(), FIRST, " ").is_err());
        assert!(rename(home.path(), "../escape", "Name").is_err());
        Ok(())
    }

    #[test]
    fn a_nested_title_is_not_the_conversations() {
        let text = "group: { title: \"Group\" } title: 'Own'";
        assert_eq!(title_field(text).map(|(_, title)| title).as_deref(), Some("Own"));
        assert_eq!(
            with_title("group: { title: \"G\" }", "N"),
            "title:\"N\"\ngroup: { title: \"G\" }"
        );
    }

    #[test]
    fn opening_resumes_and_starting_lets_agy_choose_the_id() -> Result<()> {
        let folder = tempfile::tempdir()?;
        let key = SessionKey { agent: Agent::Antigravity, id: FIRST.into() };
        let session =
            Session { key, title: "Any".into(), folder: Some(folder.path().into()), updated: 0 };

        let launch = open(&session)?;

        assert_eq!(
            (launch.program.as_str(), launch.args.as_slice()),
            ("agy", ["--conversation", FIRST].map(String::from).as_slice())
        );
        assert!(start(folder.path())?.args.is_empty());
        assert!(start(&folder.path().join("gone")).is_err());
        Ok(())
    }

    #[test]
    fn a_pane_of_the_other_kind_gets_the_counterpart_scheme() {
        assert_eq!(
            counterpart("colorblind-friendly dark", true).as_deref(),
            Some("colorblind-friendly light")
        );
        assert_eq!(counterpart("light", false).as_deref(), Some("dark"));
        assert_eq!(counterpart("colorblind-friendly dark", false), None);
        assert_eq!(counterpart("terminal", true), None);
        assert_eq!(counterpart("moonlight", false), None);
    }
}
