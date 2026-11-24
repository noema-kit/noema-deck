//! The universe as the interface draws it.

use std::collections::HashMap;

use anyhow::Result;
use serde::Serialize;

use crate::{
    folder,
    session::{Session, SessionKey},
    star::StarId,
    store::Store,
};

/// Everything on the map at one moment.
#[derive(Debug, Serialize)]
pub struct Snapshot {
    pub stars: Vec<StarView>,
    /// Every link as `[larger, smaller]`.
    pub links: Vec<(StarId, StarId)>,
    /// Every session the agent lists, most recent first. One with no stars is
    /// unsorted.
    pub sessions: Vec<SessionView>,
    /// Placed sessions the agent no longer lists.
    pub missing: Vec<MissingSession>,
    /// The folder the last new session was started in.
    pub last_folder: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct StarView {
    pub id: StarId,
    pub name: String,
    pub x: f64,
    pub y: f64,
    /// The distinct sessions on this star and on every star beneath it.
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
    /// The stars this session sits on directly.
    pub stars: Vec<StarId>,
}

#[derive(Debug, Serialize)]
pub struct MissingSession {
    #[serde(flatten)]
    pub key: SessionKey,
    pub stars: Vec<StarId>,
}

impl Snapshot {
    /// Joins what the store holds with the sessions the agent listed.
    pub(super) fn of(store: &Store, listed: &[Session]) -> Result<Self> {
        let updated: HashMap<_, _> =
            listed.iter().map(|session| (&session.key, session.updated)).collect();
        let stars = store
            .stars()?
            .into_iter()
            .map(|star| {
                let sessions = store.sessions_on(star.id, true)?;
                let latest = sessions.iter().filter_map(|key| updated.get(key).copied()).max();
                let (id, name, x, y) = (star.id, star.name, star.x, star.y);
                Ok(StarView { id, name, x, y, sessions, updated: latest })
            })
            .collect::<Result<_>>()?;

        let mut stars_of = HashMap::<SessionKey, Vec<StarId>>::new();
        for (star, session) in store.placements()? {
            stars_of.entry(session).or_default().push(star);
        }
        let sessions = listed
            .iter()
            .map(|session| SessionView {
                key: session.key.clone(),
                title: session.title.clone(),
                folder: session.folder.as_deref().map(folder::label),
                updated: session.updated,
                stars: stars_of.remove(&session.key).unwrap_or_default(),
            })
            .collect();
        // Whatever is left was placed but is no longer listed by its agent.
        let mut missing: Vec<_> =
            stars_of.into_iter().map(|(key, stars)| MissingSession { key, stars }).collect();
        missing.sort_by(|a, b| a.key.cmp(&b.key));

        Ok(Self {
            stars,
            links: store.links()?,
            sessions,
            missing,
            last_folder: store.last_folder()?.as_deref().map(folder::label),
        })
    }
}
