//! Claude Code adapter.
//!
//! Claude Code keeps one transcript per session at
//! `<config>/projects/<project>/<session-id>.jsonl`. The format is undocumented,
//! so only the few fields in [`Record`] are read.

use std::{
    collections::HashMap,
    env,
    fs::{self, File},
    io::{BufRead, BufReader},
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use anyhow::{Context, Result, ensure};
use serde::Deserialize;

use super::{Agent, Launch, append, clean_title, entries, is_uuid};
use crate::session::{Session, SessionKey};

/// Claude Code's own folder: `$CLAUDE_CONFIG_DIR`, else `~/.claude`.
fn config_dir() -> Result<PathBuf> {
    match env::var_os("CLAUDE_CONFIG_DIR") {
        Some(dir) if !dir.is_empty() => Ok(dir.into()),
        _ => Ok(env::home_dir().context("No home directory")?.join(".claude")),
    }
}

/// Where Claude Code stores transcripts.
pub fn projects_dir() -> Result<PathBuf> {
    Ok(config_dir()?.join("projects"))
}

/// Claude Code's state file: `.claude.json` in `$CLAUDE_CONFIG_DIR`, else in
/// the home folder, beside `~/.claude`.
pub fn state_file() -> Result<PathBuf> {
    match env::var_os("CLAUDE_CONFIG_DIR") {
        Some(dir) if !dir.is_empty() => Ok(PathBuf::from(dir).join(".claude.json")),
        _ => Ok(env::home_dir().context("No home directory")?.join(".claude.json")),
    }
}

/// Remembers each transcript as last read. Transcripts run to megabytes and
/// most sit idle, so listing again reads only the ones that changed.
#[derive(Default)]
pub struct Listing {
    read: HashMap<PathBuf, (Stamp, Session)>,
}

/// A transcript's modification time and length. A change to either means it
/// has to be read again.
type Stamp = (SystemTime, u64);

impl Listing {
    /// Every session under `projects`, most recent first. A machine where
    /// Claude Code has never run has no such folder and no sessions.
    pub fn sessions(&mut self, projects: &Path) -> Result<Vec<Session>> {
        let mut found = HashMap::new();
        let projects = entries(projects)?;
        // Sub-agent transcripts sit in sub-folders, so one level is enough.
        for project in projects.iter().filter(|project| project.is_dir()) {
            for transcript in entries(project)? {
                let Some(id) = session_id(&transcript) else { continue };
                let unread = || format!("Read {}", transcript.display());
                let metadata = fs::metadata(&transcript).with_context(unread)?;
                let stamp = (metadata.modified()?, metadata.len());
                let known = self.read.remove(&transcript).filter(|(read, _)| *read == stamp);
                let session = match known {
                    Some((_, session)) => session,
                    None => read_session(&transcript, id, stamp.0).with_context(unread)?,
                };
                found.insert(transcript, (stamp, session));
            }
        }
        self.read = found;
        let mut sessions: Vec<_> = self.read.values().map(|(_, session)| session.clone()).collect();
        sessions.sort_by(|a, b| b.updated.cmp(&a.updated).then_with(|| a.key.cmp(&b.key)));
        Ok(sessions)
    }

    /// The transcript of session `id`, as found by the last listing.
    #[must_use]
    pub fn transcript(&self, id: &str) -> Option<&Path> {
        let found = self.read.iter().find(|(_, (_, session))| session.key.id == id);
        found.map(|(path, _)| path.as_path())
    }
}

/// Names a session the way Claude Code's own `/rename` does: by appending a
/// `custom-title` record to its transcript. Naming is not activity, so the
/// transcript keeps its modification time. Answers with the name as stored.
pub fn rename(transcript: &Path, id: &str, title: &str) -> Result<String> {
    let title = clean_title(title).context("Enter a name for the session")?;
    let record =
        serde_json::json!({ "type": "custom-title", "customTitle": title, "sessionId": id });
    let modified = fs::metadata(transcript)
        .and_then(|metadata| metadata.modified())
        .with_context(|| format!("Read {}", transcript.display()))?;
    append(transcript, &record)?;
    File::options().write(true).open(transcript)?.set_modified(modified)?;
    Ok(title)
}

/// The command that reopens `session` in Claude Code's own interface.
pub fn open(session: &Session) -> Result<Launch> {
    let id = &session.key.id;
    let folder = session.folder.clone().with_context(|| format!("Session {id} has no folder"))?;
    ensure!(folder.is_dir(), "Folder {} of session {id} no longer exists", folder.display());
    Ok(Launch { folder, program: program(), args: vec!["--resume".into(), id.clone()] })
}

/// The command that starts a new session in `folder` under an ID the caller
/// chose, so the caller knows the session before Claude Code has stored it,
/// and under `name` when one is given.
pub fn start(id: &str, folder: &Path, name: Option<&str>) -> Result<Launch> {
    ensure!(folder.is_dir(), "{} is not a folder", folder.display());
    let mut args = vec!["--session-id".into(), id.into()];
    if let Some(name) = name.and_then(clean_title) {
        args.extend(["--name".into(), name]);
    }
    Ok(Launch { folder: folder.into(), program: program(), args })
}

/// Fits `launch` to a pane, whose terminal is light or dark. A person whose
/// Claude Code theme is of the other kind gets its counterpart, for this run
/// only. Their own settings are left alone.
#[must_use]
pub fn for_pane(mut launch: Launch, light: bool) -> Launch {
    if let Some(theme) = config_dir().ok().and_then(|config| counterpart(&config, light)) {
        let settings = serde_json::json!({ "theme": theme });
        launch.args.extend(["--settings".into(), settings.to_string()]);
    }
    launch
}

/// The theme matching the one chosen in the Claude Code folder `config` for
/// a light or a dark terminal, when the chosen one is of the other kind.
/// Claude Code is dark unless a theme is chosen.
fn counterpart(config: &Path, light: bool) -> Option<String> {
    // The theme is in the settings file, or in the older file beside the folder.
    let beside = config.with_file_name(".claude.json");
    let chosen = [config.join("settings.json"), beside].iter().find_map(|file| theme(file));
    let chosen = chosen.unwrap_or_else(|| "dark".into());
    let (from, to) = if light { ("dark", "light") } else { ("light", "dark") };
    Some(format!("{to}{}", chosen.strip_prefix(from)?))
}

/// The theme a Claude Code settings file names, if it can be read and has one.
fn theme(settings: &Path) -> Option<String> {
    #[derive(Deserialize)]
    struct Settings {
        theme: String,
    }
    let settings: Settings = serde_json::from_reader(File::open(settings).ok()?).ok()?;
    Some(settings.theme)
}

/// The Claude Code executable: `$NOEMA_DECK_CLAUDE` when set, else `claude`
/// wherever the `PATH` finds it.
pub(crate) fn program() -> String {
    super::program("NOEMA_DECK_CLAUDE", "claude")
}

/// The session ID of a transcript, which Claude Code names `<uuid>.jsonl`.
fn session_id(transcript: &Path) -> Option<&str> {
    if transcript.extension()? != "jsonl" || !transcript.is_file() {
        return None;
    }
    let id = transcript.file_stem()?.to_str()?;
    is_uuid(id).then_some(id)
}

/// The transcript fields Deck reads. Each appears on only some record types.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Record {
    cwd: Option<PathBuf>,
    custom_title: Option<String>,
    ai_title: Option<String>,
    last_prompt: Option<String>,
}

const TITLE_FIELDS: [&str; 3] = ["\"customTitle\"", "\"aiTitle\"", "\"lastPrompt\""];

/// A prompt the person typed, read only while a transcript has no title: a
/// session closed early, or one that only ran commands such as `/login`.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Typed {
    #[serde(default)]
    is_meta: bool,
    message: Message,
}

#[derive(Deserialize)]
struct Message {
    content: String,
}

/// What a typed message offers as a title: its text, or the command it ran.
/// Claude Code wraps a command and its output in tags.
enum Offer {
    Prompt(String),
    Command(String),
}

fn offer(typed: Typed) -> Option<Offer> {
    let text = typed.message.content;
    if typed.is_meta {
        return None;
    }
    if let Some(rest) = text.trim_start().strip_prefix("<command-name>") {
        return rest.split_once("</command-name>").map(|(name, _)| Offer::Command(name.into()));
    }
    (!text.trim_start().starts_with('<')).then_some(Offer::Prompt(text))
}

fn read_session(transcript: &Path, id: &str, modified: SystemTime) -> Result<Session> {
    let since_epoch = modified.duration_since(UNIX_EPOCH).ok();
    let updated = since_epoch.and_then(|age| i64::try_from(age.as_secs()).ok()).unwrap_or(0);

    let (mut folder, mut custom, mut ai, mut prompt) = (None, None, None, None);
    let (mut typed, mut command) = (None, None);
    let mut reader = BufReader::new(File::open(transcript)?);
    let mut line = Vec::new();
    while reader.read_until(b'\n', &mut line)? != 0 {
        // Lines run to megabytes, so parse only those that can hold a wanted field.
        if let Ok(text) = std::str::from_utf8(&line)
            && (folder.is_none() && text.contains("\"cwd\"")
                || TITLE_FIELDS.iter().any(|field| text.contains(field)))
            && let Ok(record) = serde_json::from_str::<Record>(text)
        {
            folder = folder.or(record.cwd);
            custom = record.custom_title.or(custom);
            ai = record.ai_title.or(ai);
            prompt = record.last_prompt.or(prompt);
        }
        if typed.is_none()
            && let Ok(text) = std::str::from_utf8(&line)
            && text.contains("\"type\":\"user\"")
            && let Ok(record) = serde_json::from_str::<Typed>(text)
        {
            match offer(record) {
                Some(Offer::Prompt(text)) => typed = Some(text),
                Some(Offer::Command(name)) => command = command.or(Some(name)),
                None => {}
            }
        }
        line.clear();
    }

    let title = [custom, ai, prompt, typed, command]
        .into_iter()
        .flatten()
        .find_map(|text| {
            text.lines().map(str::trim).find(|line| !line.is_empty()).map(String::from)
        })
        .unwrap_or_else(|| "Untitled".into());
    let key = SessionKey { agent: Agent::Claude, id: id.into() };
    Ok(Session { key, title, folder, updated })
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, SystemTime};

    use super::*;

    const FIRST: &str = "11111111-1111-4111-8111-111111111111";
    const SECOND: &str = "22222222-2222-4222-8222-222222222222";

    fn transcript(project: &Path, name: &str, lines: &[&str], modified: u64) -> Result<()> {
        fs::create_dir_all(project)?;
        let path = project.join(name);
        fs::write(&path, lines.join("\n"))?;
        File::options()
            .write(true)
            .open(path)?
            .set_modified(SystemTime::UNIX_EPOCH + Duration::from_secs(modified))?;
        Ok(())
    }

    #[test]
    fn lists_sessions_across_projects_most_recent_first() -> Result<()> {
        let root = tempfile::tempdir()?;
        let lines =
            [r#"{"type":"user","cwd":"/work/a"}"#, r#"{"type":"ai-title","aiTitle":"Older"}"#];
        transcript(&root.path().join("a"), &format!("{FIRST}.jsonl"), &lines, 100)?;
        let lines =
            [r#"{"type":"user","cwd":"/work/b"}"#, r#"{"type":"ai-title","aiTitle":"Newer"}"#];
        transcript(&root.path().join("b"), &format!("{SECOND}.jsonl"), &lines, 200)?;

        let sessions = Listing::default().sessions(root.path())?;

        let seen: Vec<_> =
            sessions.iter().map(|s| (s.key.id.as_str(), s.title.as_str(), s.updated)).collect();
        assert_eq!(seen, [(SECOND, "Newer", 200), (FIRST, "Older", 100)]);
        assert_eq!(sessions[0].folder.as_deref(), Some(Path::new("/work/b")));
        Ok(())
    }

    #[test]
    fn title_prefers_the_last_custom_title_then_ai_title_then_prompt() -> Result<()> {
        let root = tempfile::tempdir()?;
        let project = root.path().join("p");
        let lines = [
            r#"{"type":"last-prompt","lastPrompt":"\n  first line of prompt\nsecond"}"#,
            r#"{"type":"ai-title","aiTitle":"Generated"}"#,
            r#"{"type":"custom-title","customTitle":"Named"}"#,
            r#"{"type":"custom-title","customTitle":"Renamed"}"#,
        ];
        transcript(&project, &format!("{FIRST}.jsonl"), &lines, 3)?;
        transcript(&project, &format!("{SECOND}.jsonl"), &lines[..1], 2)?;

        let titles: Vec<_> =
            Listing::default().sessions(root.path())?.into_iter().map(|s| s.title).collect();

        assert_eq!(titles, ["Renamed", "first line of prompt"]);
        Ok(())
    }

    #[test]
    fn without_a_title_the_first_typed_prompt_or_command_names_a_session() -> Result<()> {
        let root = tempfile::tempdir()?;
        let project = root.path().join("p");
        let caveat =
            r#"{"type":"user","isMeta":true,"message":{"role":"user","content":"Typed by hand"}}"#;
        let command = r#"{"type":"user","message":{"role":"user","content":"<command-name>/login</command-name>\n<command-args></command-args>"}}"#;
        let output = r#"{"type":"user","message":{"role":"user","content":"<local-command-stdout>Done</local-command-stdout>"}}"#;
        let prompt =
            r#"{"type":"user","message":{"role":"user","content":"\n  Fix the build\nthen test"}}"#;
        let result =
            r#"{"type":"user","message":{"role":"user","content":[{"type":"tool_result"}]}}"#;
        transcript(
            &project,
            &format!("{FIRST}.jsonl"),
            &[caveat, command, output, result, prompt],
            2,
        )?;
        transcript(&project, &format!("{SECOND}.jsonl"), &[caveat, command, output], 1)?;

        let titles: Vec<_> =
            Listing::default().sessions(root.path())?.into_iter().map(|s| s.title).collect();

        assert_eq!(titles, ["Fix the build", "/login"]);
        Ok(())
    }

    #[test]
    fn renaming_appends_a_custom_title_and_keeps_the_time() -> Result<()> {
        let root = tempfile::tempdir()?;
        let project = root.path().join("p");
        // The last line has no newline, as if cut off.
        transcript(
            &project,
            &format!("{FIRST}.jsonl"),
            &[r#"{"type":"ai-title","aiTitle":"Old"}"#],
            100,
        )?;
        let mut listing = Listing::default();
        listing.sessions(root.path())?;
        let path = listing.transcript(FIRST).context("transcript")?.to_owned();

        let stored = rename(&path, FIRST, "  Deck\n  rename ")?;

        assert_eq!(stored, "Deck rename");
        let sessions = listing.sessions(root.path())?;
        assert_eq!((sessions[0].title.as_str(), sessions[0].updated), ("Deck rename", 100));
        let text = fs::read_to_string(&path)?;
        let last: serde_json::Value = serde_json::from_str(text.lines().last().context("line")?)?;
        assert_eq!(last["sessionId"], FIRST);
        assert!(text.ends_with('\n') && text.lines().count() == 2);
        assert!(rename(&path, FIRST, " \n ").is_err());
        assert!(listing.transcript(SECOND).is_none());
        Ok(())
    }

    #[test]
    fn folder_is_the_first_cwd_and_broken_lines_are_skipped() -> Result<()> {
        let root = tempfile::tempdir()?;
        let lines = [
            r#"{"type":"queue-operation","content":"mentions \"cwd\" only in text"}"#,
            r#"{"type":"user","cwd":"/work/start"#,
            r#"{"type":"user","cwd":"/work/start"}"#,
            r#"{"type":"user","cwd":"/work/elsewhere"}"#,
        ];
        transcript(&root.path().join("p"), &format!("{FIRST}.jsonl"), &lines, 1)?;

        let sessions = Listing::default().sessions(root.path())?;

        assert_eq!(sessions[0].folder.as_deref(), Some(Path::new("/work/start")));
        assert_eq!(sessions[0].title, "Untitled");
        Ok(())
    }

    #[test]
    fn a_transcript_is_read_again_only_when_it_changes() -> Result<()> {
        let root = tempfile::tempdir()?;
        let (project, name) = (root.path().join("p"), format!("{FIRST}.jsonl"));
        transcript(&project, &name, &[r#"{"type":"ai-title","aiTitle":"Before"}"#], 100)?;
        let mut listing = Listing::default();
        assert_eq!(listing.sessions(root.path())?[0].title, "Before");

        // The same length and time: nothing says it changed, so it is not read.
        transcript(&project, &name, &[r#"{"type":"ai-title","aiTitle":"Edited"}"#], 100)?;
        assert_eq!(listing.sessions(root.path())?[0].title, "Before");

        transcript(&project, &name, &[r#"{"type":"ai-title","aiTitle":"Edited"}"#], 200)?;
        let sessions = listing.sessions(root.path())?;
        assert_eq!((sessions[0].title.as_str(), sessions[0].updated), ("Edited", 200));

        fs::remove_file(project.join(&name))?;
        assert!(listing.sessions(root.path())?.is_empty());
        Ok(())
    }

    #[test]
    fn a_theme_of_the_other_kind_has_a_counterpart_for_panes() -> Result<()> {
        let root = tempfile::tempdir()?;
        let config = root.path().join("claude");
        fs::create_dir(&config)?;
        assert_eq!(counterpart(&config, false), None);
        assert_eq!(counterpart(&config, true).as_deref(), Some("light"));

        fs::write(root.path().join(".claude.json"), r#"{"theme":"light"}"#)?;
        assert_eq!(counterpart(&config, false).as_deref(), Some("dark"));
        assert_eq!(counterpart(&config, true), None);
        fs::write(config.join("settings.json"), r#"{"theme":"light-daltonized","other":1}"#)?;
        assert_eq!(counterpart(&config, false).as_deref(), Some("dark-daltonized"));
        fs::write(config.join("settings.json"), r#"{"theme":"dark-ansi"}"#)?;
        assert_eq!(counterpart(&config, false), None);
        assert_eq!(counterpart(&config, true).as_deref(), Some("light-ansi"));
        Ok(())
    }

    #[test]
    fn starting_passes_the_chosen_id_and_needs_a_folder() -> Result<()> {
        let folder = tempfile::tempdir()?;

        let launch = start(FIRST, folder.path(), None)?;

        assert_eq!(launch.args, ["--session-id", FIRST]);
        assert_eq!(launch.folder, folder.path());
        assert!(start(FIRST, &folder.path().join("gone"), None).is_err());
        let named = start(FIRST, folder.path(), Some(" Fix  the build "))?;
        assert_eq!(named.args, ["--session-id", FIRST, "--name", "Fix the build"]);
        assert_eq!(start(FIRST, folder.path(), Some("  "))?.args.len(), 2);
        Ok(())
    }

    #[test]
    fn only_uuid_named_transcripts_at_project_level_are_sessions() -> Result<()> {
        let root = tempfile::tempdir()?;
        let project = root.path().join("p");
        let line = [r#"{"type":"user","cwd":"/work"}"#];
        transcript(&project, &format!("{FIRST}.jsonl"), &line, 1)?;
        transcript(&project, "notes.jsonl", &line, 1)?;
        transcript(&project, &format!("{SECOND}.json"), &line, 1)?;
        transcript(&project.join(FIRST).join("subagents"), &format!("{SECOND}.jsonl"), &line, 1)?;
        fs::write(root.path().join("stray.jsonl"), "")?;

        let ids: Vec<_> =
            Listing::default().sessions(root.path())?.into_iter().map(|s| s.key.id).collect();

        assert_eq!(ids, [FIRST]);
        Ok(())
    }

    #[test]
    fn opening_resumes_the_session_in_its_folder() -> Result<()> {
        let folder = tempfile::tempdir()?;
        let key = SessionKey { agent: Agent::Claude, id: FIRST.into() };
        let mut session =
            Session { key, title: "Any".into(), folder: Some(folder.path().into()), updated: 0 };

        let launch = open(&session)?;

        assert_eq!(launch.program, "claude");
        assert_eq!(launch.args, ["--resume", FIRST]);
        assert_eq!(launch.folder, folder.path());

        session.folder = Some(folder.path().join("gone"));
        assert!(open(&session).is_err());
        session.folder = None;
        assert!(open(&session).is_err());
        Ok(())
    }
}
