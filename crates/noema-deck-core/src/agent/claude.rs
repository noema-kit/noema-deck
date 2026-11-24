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

use super::{Agent, Launch};
use crate::session::{Session, SessionKey};

/// Where Claude Code stores transcripts: `$CLAUDE_CONFIG_DIR/projects`, else
/// `~/.claude/projects`.
pub fn projects_dir() -> Result<PathBuf> {
    let config = match env::var_os("CLAUDE_CONFIG_DIR") {
        Some(dir) if !dir.is_empty() => PathBuf::from(dir),
        _ => env::home_dir().context("No home directory")?.join(".claude"),
    };
    Ok(config.join("projects"))
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
        let projects = if projects.exists() { entries(projects)? } else { Vec::new() };
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
}

/// The command that reopens `session` in Claude Code's own interface.
pub fn open(session: &Session) -> Result<Launch> {
    let id = &session.key.id;
    let folder = session.folder.clone().with_context(|| format!("Session {id} has no folder"))?;
    ensure!(folder.is_dir(), "Folder {} of session {id} no longer exists", folder.display());
    Ok(Launch { folder, program: program(), args: vec!["--resume".into(), id.clone()] })
}

/// The command that starts a new session in `folder` under an ID the caller
/// chose, so the caller knows the session before Claude Code has stored it.
pub fn start(id: &str, folder: &Path) -> Result<Launch> {
    ensure!(folder.is_dir(), "{} is not a folder", folder.display());
    Ok(Launch {
        folder: folder.into(),
        program: program(),
        args: vec!["--session-id".into(), id.into()],
    })
}

/// The Claude Code executable: `$NOEMA_DECK_CLAUDE` when set, else `claude`
/// wherever the `PATH` finds it.
fn program() -> String {
    let named = env::var("NOEMA_DECK_CLAUDE").ok().filter(|program| !program.is_empty());
    named.unwrap_or_else(|| "claude".into())
}

fn entries(dir: &Path) -> Result<Vec<PathBuf>> {
    fs::read_dir(dir)
        .with_context(|| format!("Read {}", dir.display()))?
        .map(|entry| Ok(entry?.path()))
        .collect()
}

/// The session ID of a transcript, which Claude Code names `<uuid>.jsonl`.
fn session_id(transcript: &Path) -> Option<&str> {
    if transcript.extension()? != "jsonl" || !transcript.is_file() {
        return None;
    }
    let id = transcript.file_stem()?.to_str()?;
    let is_uuid = id.len() == 36
        && id.bytes().enumerate().all(|(index, byte)| match index {
            8 | 13 | 18 | 23 => byte == b'-',
            _ => byte.is_ascii_hexdigit(),
        });
    is_uuid.then_some(id)
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

fn read_session(transcript: &Path, id: &str, modified: SystemTime) -> Result<Session> {
    let since_epoch = modified.duration_since(UNIX_EPOCH).ok();
    let updated = since_epoch.and_then(|age| i64::try_from(age.as_secs()).ok()).unwrap_or(0);

    let (mut folder, mut custom, mut ai, mut prompt) = (None, None, None, None);
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
        line.clear();
    }

    let title = [custom, ai, prompt]
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
    fn starting_passes_the_chosen_id_and_needs_a_folder() -> Result<()> {
        let folder = tempfile::tempdir()?;

        let launch = start(FIRST, folder.path())?;

        assert_eq!(launch.args, ["--session-id", FIRST]);
        assert_eq!(launch.folder, folder.path());
        assert!(start(FIRST, &folder.path().join("gone")).is_err());
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
