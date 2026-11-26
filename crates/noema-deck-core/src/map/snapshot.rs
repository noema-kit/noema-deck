//! The map as the interface draws it.

use std::collections::HashMap;

use anyhow::Result;
use serde::Serialize;

use crate::{
    domain::DomainId,
    folder,
    session::{Session, SessionKey},
    store::{Appearance, OpenPanes, Shortcuts, Store},
};

/// Everything on the map at one moment.
#[derive(Debug, Serialize)]
pub struct Snapshot {
    pub domains: Vec<DomainView>,
    /// Every link as `[parent, child]`.
    pub links: Vec<(DomainId, DomainId)>,
    /// Every session the agent lists, most recent first. One with no domains is
    /// unassigned.
    pub sessions: Vec<SessionView>,
    /// Assigned sessions the agent no longer lists.
    pub missing: Vec<MissingSession>,
    /// The folder the last new session was started in.
    pub last_folder: Option<String>,
    /// The panes open when Deck last stopped.
    pub panes: OpenPanes,
    pub appearance: Appearance,
    /// The shortcuts moved to other keys.
    pub shortcuts: Shortcuts,
}

#[derive(Debug, Serialize)]
pub struct DomainView {
    pub id: DomainId,
    pub name: String,
    pub x: f64,
    pub y: f64,
    /// The distinct sessions on this domain and on every subdomain.
    pub sessions: Vec<SessionKey>,
    /// The most recent activity among those sessions.
    pub updated: Option<i64>,
}

#[derive(Debug, Serialize)]
pub struct SessionView {
    #[serde(flatten)]
    pub key: SessionKey,
    pub title: String,
    pub folder: Option<String>,
    pub updated: i64,
    /// The domains this session belongs to directly.
    pub domains: Vec<DomainId>,
}

#[derive(Debug, Serialize)]
pub struct MissingSession {
    #[serde(flatten)]
    pub key: SessionKey,
    pub domains: Vec<DomainId>,
}

impl Snapshot {
    /// Joins what the store holds with the sessions the agent listed.
    pub(super) fn of(store: &Store, listed: &[Session]) -> Result<Self> {
        let updated: HashMap<_, _> =
            listed.iter().map(|session| (&session.key, session.updated)).collect();
        let domains = store
            .domains()?
            .into_iter()
            .map(|domain| {
                let sessions = store.sessions_on(domain.id, true)?;
                let latest = sessions.iter().filter_map(|key| updated.get(key).copied()).max();
                let (id, name, x, y) = (domain.id, domain.name, domain.x, domain.y);
                Ok(DomainView { id, name, x, y, sessions, updated: latest })
            })
            .collect::<Result<_>>()?;

        let mut domains_of = HashMap::<SessionKey, Vec<DomainId>>::new();
        for (domain, session) in store.assignments()? {
            domains_of.entry(session).or_default().push(domain);
        }
        let sessions = listed
            .iter()
            .map(|session| SessionView {
                key: session.key.clone(),
                title: session.title.clone(),
                folder: session.folder.as_deref().map(folder::label),
                updated: session.updated,
                domains: domains_of.remove(&session.key).unwrap_or_default(),
            })
            .collect();
        // Whatever is left was assigned but is no longer listed by its agent.
        let mut missing: Vec<_> =
            domains_of.into_iter().map(|(key, domains)| MissingSession { key, domains }).collect();
        missing.sort_by(|a, b| a.key.cmp(&b.key));

        Ok(Self {
            domains,
            links: store.links()?,
            sessions,
            missing,
            last_folder: store.last_folder()?.as_deref().map(folder::label),
            panes: store.open_panes()?,
            appearance: store.appearance()?,
            shortcuts: store.shortcuts()?,
        })
    }
}
