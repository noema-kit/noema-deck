//! The agents whose sessions Deck arranges, and how each one is run.

pub mod claude;
pub mod codex;

use std::{
    fmt,
    fs::{self, File},
    io::{Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
    str::FromStr,
};

use anyhow::{Context, Error, Result, bail};
use serde::{Deserialize, Serialize};

use crate::session::{Session, SessionKey};

/// An agent whose sessions Deck can assign to domains.
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize,
)]
#[serde(rename_all = "lowercase")]
pub enum Agent {
    #[default]
    Claude,
    Codex,
}

impl Agent {
    /// The name the store and the interface know the agent by.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Claude => "claude",
            Self::Codex => "codex",
        }
    }
}

impl fmt::Display for Agent {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.name())
    }
}

impl FromStr for Agent {
    type Err = Error;

    fn from_str(name: &str) -> Result<Self, Self::Err> {
        match name {
            "claude" => Ok(Self::Claude),
            "codex" => Ok(Self::Codex),
            other => bail!("Unknown agent `{other}`"),
        }
    }
}

/// A command to run in a folder.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Launch {
    pub folder: PathBuf,
    pub program: String,
    pub args: Vec<String>,
}

/// Where each agent keeps its sessions.
#[derive(Clone, Debug)]
pub struct Homes {
    /// Claude Code's transcripts, one folder per project.
    pub claude: PathBuf,
    /// Codex's own folder.
    pub codex: PathBuf,
}

impl Homes {
    /// The folders the agents use on this machine.
    pub fn find() -> Result<Self> {
        Ok(Self { claude: claude::projects_dir()?, codex: codex::home()? })
    }
}

/// Every agent's sessions, as last read.
#[derive(Default)]
pub struct Listing {
    claude: claude::Listing,
    codex: codex::Listing,
}

impl Listing {
    /// Every agent's sessions, most recent first.
    pub fn sessions(&mut self, homes: &Homes) -> Result<Vec<Session>> {
        let mut sessions = self.claude.sessions(&homes.claude)?;
        sessions.extend(self.codex.sessions(&homes.codex)?);
        sessions.sort_by(|a, b| b.updated.cmp(&a.updated).then_with(|| a.key.cmp(&b.key)));
        Ok(sessions)
    }

    /// Names a listed session where its agent keeps names, so the agent shows
    /// the same name. Answers with the name as stored.
    pub fn rename(&self, homes: &Homes, key: &SessionKey, title: &str) -> Result<String> {
        match key.agent {
            Agent::Claude => {
                let transcript =
                    self.claude.transcript(&key.id).context("That session is no longer listed")?;
                claude::rename(transcript, &key.id, title)
            }
            Agent::Codex => codex::rename(&homes.codex, &key.id, title),
        }
    }
}

/// The command that reopens `session` in its agent's own interface.
pub fn open(session: &Session) -> Result<Launch> {
    match session.key.agent {
        Agent::Claude => claude::open(session),
        Agent::Codex => codex::open(session),
    }
}

/// Fits `agent`'s `launch` to a pane whose terminal is light or dark. Codex
/// asks the terminal for its colours itself.
#[must_use]
pub fn for_pane(agent: Agent, launch: Launch, light: bool) -> Launch {
    match agent {
        Agent::Claude => claude::for_pane(launch, light),
        Agent::Codex => launch,
    }
}

/// A title on one line with single spaces, or `None` when nothing is left.
#[must_use]
pub fn clean_title(title: &str) -> Option<String> {
    let title = title.split_whitespace().collect::<Vec<_>>().join(" ");
    (!title.is_empty()).then_some(title)
}

/// The executable `variable` names, else `program` wherever the `PATH` finds it.
fn program(variable: &str, program: &str) -> String {
    let named = std::env::var(variable).ok().filter(|program| !program.is_empty());
    named.unwrap_or_else(|| program.into())
}

/// Adds `record` as a line at the end of `file`, creating it if need be. A
/// file cut off mid-line must not swallow the record.
fn append(file: &Path, record: &serde_json::Value) -> Result<()> {
    let unwritten = || format!("Write {}", file.display());
    let mut file =
        File::options().read(true).append(true).create(true).open(file).with_context(unwritten)?;
    let mut last = [b'\n'];
    if file.metadata()?.len() > 0 {
        file.seek(SeekFrom::End(-1))?;
        file.read_exact(&mut last)?;
    }
    let lead = if last[0] == b'\n' { "" } else { "\n" };
    file.write_all(format!("{lead}{record}\n").as_bytes()).with_context(unwritten)
}

/// The entries of `dir`, or none when it does not exist.
fn entries(dir: &Path) -> Result<Vec<PathBuf>> {
    if !dir.exists() {
        return Ok(Vec::new());
    }
    fs::read_dir(dir)
        .with_context(|| format!("Read {}", dir.display()))?
        .map(|entry| Ok(entry?.path()))
        .collect()
}

/// Whether `id` is a UUID written out in full.
fn is_uuid(id: &str) -> bool {
    id.len() == 36
        && id.bytes().enumerate().all(|(index, byte)| match index {
            8 | 13 | 18 | 23 => byte == b'-',
            _ => byte.is_ascii_hexdigit(),
        })
}
