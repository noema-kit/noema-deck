//! What the interface can ask of the universe.

use serde::Deserialize;

use crate::{session::SessionKey, star::StarId};

/// A request about the map. Every one is answered with a fresh
/// [`Snapshot`](super::Snapshot).
#[derive(Debug, Deserialize)]
#[serde(tag = "command", content = "args", rename_all = "snake_case")]
pub enum Request {
    Snapshot,
    /// Re-read the sessions from the agent.
    Refresh,
    AddStar {
        name: String,
        larger: Option<StarId>,
        position: Option<(f64, f64)>,
    },
    /// Add a folder and the folders inside it as a tree of stars.
    Import {
        folder: String,
        larger: Option<StarId>,
        position: Option<(f64, f64)>,
        depth: Option<usize>,
    },
    RenameStar {
        star: StarId,
        name: String,
    },
    MoveStar {
        star: StarId,
        x: f64,
        y: f64,
    },
    DeleteStar {
        star: StarId,
    },
    Link {
        larger: StarId,
        smaller: StarId,
    },
    Unlink {
        larger: StarId,
        smaller: StarId,
    },
    Place {
        star: StarId,
        sessions: Vec<SessionKey>,
    },
    Unplace {
        star: StarId,
        session: SessionKey,
    },
}
