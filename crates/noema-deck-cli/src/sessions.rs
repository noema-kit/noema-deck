//! Commands about sessions.

use std::{collections::HashMap, os::unix::process::CommandExt, process};

use anyhow::{Context, Result};
use noema_deck_core::{
    Map, agent,
    map::{Request, SessionView},
    search::Texts,
};

use crate::{find, print};

/// Prints the sessions, optionally only the unassigned ones or those matching
/// `query`.
pub fn list(map: &Map, unassigned: bool, query: Option<&str>) -> Result<()> {
    let snapshot = map.snapshot()?;
    let query = query.map(str::to_lowercase);
    let matches = |session: &SessionView| {
        let folder = session.folder.as_deref().unwrap_or_default();
        query.as_deref().is_none_or(|text| {
            session.title.to_lowercase().contains(text) || folder.to_lowercase().contains(text)
        })
    };
    let shown = snapshot
        .sessions
        .iter()
        .filter(|session| !unassigned || session.domains.is_empty())
        .filter(|session| matches(session));
    print::sessions(&snapshot, shown, |_| None);
    Ok(())
}

/// Prints the sessions in which `text` was said, each with the first place
/// it was, the phrase between brackets.
pub fn search(map: &Map, text: &str) -> Result<()> {
    let snapshot = map.snapshot()?;
    let hits = Texts::default().find(map.homes(), text)?;
    let hits: HashMap<_, _> = hits.iter().map(|hit| (&hit.key, hit)).collect();
    let shown = snapshot.sessions.iter().filter(|session| hits.contains_key(&session.key));
    print::sessions(&snapshot, shown, |session| {
        let hit = hits.get(&session.key)?;
        Some(format!("{}× {}[{}]{}", hit.count, hit.before, hit.found, hit.after))
    });
    Ok(())
}

/// Replaces this process with the session's agent, resuming it in its folder.
pub fn open(map: &Map, session: &str) -> Result<()> {
    let snapshot = map.snapshot()?;
    let key = find::session(snapshot.sessions.iter().map(|session| &session.key), session)?;
    let session = map.session(&key).context("That session is no longer listed")?;
    let launch = agent::open(session)?;
    let failure = process::Command::new(&launch.program)
        .args(&launch.args)
        .current_dir(&launch.folder)
        .exec();
    Err(failure).with_context(|| format!("Run `{}`", launch.program))
}

pub fn rename(map: &mut Map, session: &str, title: &str) -> Result<()> {
    let snapshot = map.snapshot()?;
    let session = find::session(snapshot.sessions.iter().map(|session| &session.key), session)?;
    map.handle(Request::RenameSession { session, title: title.into() })?;
    Ok(())
}

pub fn assign(map: &mut Map, session: &str, domain: &str) -> Result<()> {
    let snapshot = map.snapshot()?;
    let key = find::session(snapshot.sessions.iter().map(|session| &session.key), session)?;
    let domain = find::domain(&snapshot, domain)?;
    map.handle(Request::Assign { domain, sessions: vec![key] })?;
    Ok(())
}

/// Removes a session from a domain. A session the agent no longer lists can still
/// be named, since removing it is the way to tidy up.
pub fn unassign(map: &mut Map, session: &str, domain: &str) -> Result<()> {
    let snapshot = map.snapshot()?;
    let domain = find::domain(&snapshot, domain)?;
    let listed = snapshot.sessions.iter().map(|session| (&session.key, &session.domains));
    let missing = snapshot.missing.iter().map(|session| (&session.key, &session.domains));
    let on_domain = listed.chain(missing).filter(|(_, domains)| domains.contains(&domain));
    let session = find::session(on_domain.map(|(key, _)| key), session)?;
    map.handle(Request::Unassign { domain, session })?;
    Ok(())
}
