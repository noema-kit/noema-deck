//! What the interface can ask of the map.

use serde::Deserialize;

use crate::{
    domain::DomainId,
    session::SessionKey,
    store::{Appearance, OpenPanes, Profile, Shortcuts},
};

/// A request about the map. Every one is answered with a fresh
/// [`Snapshot`](super::Snapshot).
#[derive(Debug, Deserialize)]
#[serde(tag = "command", content = "args", rename_all = "snake_case")]
pub enum Request {
    Snapshot,
    /// Re-read the sessions from the agent.
    Refresh,
    AddDomain {
        name: String,
        parent: Option<DomainId>,
        position: Option<(f64, f64)>,
    },
    /// Add a folder and the folders inside it as a tree of domains.
    Import {
        folder: String,
        parent: Option<DomainId>,
        position: Option<(f64, f64)>,
        depth: Option<usize>,
    },
    RenameDomain {
        domain: DomainId,
        name: String,
    },
    MoveDomain {
        domain: DomainId,
        x: f64,
        y: f64,
    },
    /// Lay every domain out by depth around one centre, roots at the core.
    Arrange,
    /// Move several domains at once, as undoing an arrangement does.
    MoveDomains {
        moves: Vec<(DomainId, f64, f64)>,
    },
    DeleteDomain {
        domain: DomainId,
    },
    Link {
        parent: DomainId,
        child: DomainId,
    },
    Unlink {
        parent: DomainId,
        child: DomainId,
    },
    Assign {
        domain: DomainId,
        sessions: Vec<SessionKey>,
    },
    Unassign {
        domain: DomainId,
        session: SessionKey,
    },
    /// Name a session in its agent's own records.
    RenameSession {
        session: SessionKey,
        title: String,
    },
    /// Remember the panes open now, for the next run.
    KeepPanes {
        panes: OpenPanes,
    },
    /// Change how Deck looks.
    SetAppearance {
        appearance: Appearance,
    },
    /// Move shortcuts to other keys. The ones left out keep their own.
    SetShortcuts {
        shortcuts: Shortcuts,
    },
    /// Change the name and picture on the Profile page.
    SetProfile {
        profile: Profile,
    },
}
