//! What the interface can ask of the panes.

use serde::{Deserialize, Serialize};

use super::PaneId;
use crate::{domain::DomainId, session::SessionKey};

/// A request about a pane.
#[derive(Debug, Deserialize)]
#[serde(tag = "command", content = "args", rename_all = "snake_case")]
pub enum Request {
    /// Open a session in a new pane. Answered with the pane's ID.
    Open {
        session: SessionKey,
        rows: u16,
        cols: u16,
    },
    /// Start a new session on a domain, in a new pane. Answered with the pane's
    /// ID and the session's key.
    Start {
        domain: DomainId,
        folder: String,
        /// A name for the session. Optional.
        name: Option<String>,
        rows: u16,
        cols: u16,
    },
    /// Run the user's own shell in a folder, in a new pane. Answered with the
    /// pane's ID.
    Shell {
        folder: String,
        rows: u16,
        cols: u16,
    },
    /// Wait for output. Answered with it, and whether the process has ended.
    Read {
        pane: PaneId,
    },
    Write {
        pane: PaneId,
        data: String,
    },
    Resize {
        pane: PaneId,
        rows: u16,
        cols: u16,
    },
    /// Stop the pane's process. The session stays with its agent.
    Close {
        pane: PaneId,
    },
}

/// The answer to a [`Request`], holding only what that request asks for.
#[derive(Debug, Default, PartialEq, Eq, Serialize)]
pub struct Reply {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pane: Option<PaneId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session: Option<SessionKey>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exited: Option<bool>,
}
