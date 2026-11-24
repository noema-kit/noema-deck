//! Commands about sessions.

use std::{os::unix::process::CommandExt, process};

use anyhow::{Context, Result};
use noema_deck_core::{
    Universe,
    agent::claude,
    universe::{Request, SessionView},
};

use crate::{find, print};

/// Prints the sessions, optionally only the unsorted ones or those matching
/// `query`.
pub fn list(universe: &Universe, unsorted: bool, query: Option<&str>) -> Result<()> {
    let snapshot = universe.snapshot()?;
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
        .filter(|session| !unsorted || session.stars.is_empty())
        .filter(|session| matches(session));
    print::sessions(&snapshot, shown);
    Ok(())
}

/// Replaces this process with Claude Code, resuming the session in its folder.
pub fn open(universe: &Universe, session: &str) -> Result<()> {
    let snapshot = universe.snapshot()?;
    let key = find::session(snapshot.sessions.iter().map(|session| &session.key), session)?;
    let session = universe.session(&key).context("That session is no longer listed")?;
    let launch = claude::open(session)?;
    let failure = process::Command::new(&launch.program)
        .args(&launch.args)
        .current_dir(&launch.folder)
        .exec();
    Err(failure).with_context(|| format!("Run `{}`", launch.program))
}

pub fn place(universe: &mut Universe, session: &str, star: &str) -> Result<()> {
    let snapshot = universe.snapshot()?;
    let key = find::session(snapshot.sessions.iter().map(|session| &session.key), session)?;
    let star = find::star(&snapshot, star)?;
    universe.handle(Request::Place { star, sessions: vec![key] })?;
    Ok(())
}

/// Takes a session off a star. A session the agent no longer lists can still
/// be named, since removing it is the way to tidy up.
pub fn unplace(universe: &mut Universe, session: &str, star: &str) -> Result<()> {
    let snapshot = universe.snapshot()?;
    let star = find::star(&snapshot, star)?;
    let listed = snapshot.sessions.iter().map(|session| (&session.key, &session.stars));
    let missing = snapshot.missing.iter().map(|session| (&session.key, &session.stars));
    let on_star = listed.chain(missing).filter(|(_, stars)| stars.contains(&star));
    let session = find::session(on_star.map(|(key, _)| key), session)?;
    universe.handle(Request::Unplace { star, session })?;
    Ok(())
}
