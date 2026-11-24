//! Sessions as Deck sees them.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::agent::Agent;

/// The agent's name and the session's own ID. Deck stores nothing else about
/// a session.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct SessionKey {
    pub agent: Agent,
    pub id: String,
}

/// A session as read from its agent.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Session {
    pub key: SessionKey,
    pub title: String,
    /// Folder the session started in. Opening the session runs the agent there.
    pub folder: Option<PathBuf>,
    /// Last activity in Unix seconds.
    pub updated: i64,
}
