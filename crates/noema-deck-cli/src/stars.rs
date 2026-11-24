//! Commands about stars.

use anyhow::{Context, Result};
use noema_deck_core::{SessionKey, Snapshot, StarId, Universe, universe::Request};

use crate::{StarCommand, find, print};

pub fn run(universe: &mut Universe, command: StarCommand) -> Result<()> {
    let snapshot = universe.snapshot()?;
    let star = |text: &str| find::star(&snapshot, text);
    let request = match command {
        StarCommand::List => {
            print::stars(&snapshot);
            return Ok(());
        }
        StarCommand::Show { star: target, all } => return show(&snapshot, star(&target)?, all),
        StarCommand::Add { name, under } => {
            let larger = under.as_deref().map(star).transpose()?;
            let id = universe.add_star(&name, larger)?;
            println!("Added star [{id}] {}", name.trim());
            return Ok(());
        }
        StarCommand::Import { folder, under, depth } => {
            let larger = under.as_deref().map(star).transpose()?;
            let added = universe.import(&folder, larger, None, depth)?;
            println!("Added {added} stars from {}", folder.display());
            return Ok(());
        }
        StarCommand::Rename { star: target, name } => {
            Request::RenameStar { star: star(&target)?, name }
        }
        StarCommand::Delete { star: target } => Request::DeleteStar { star: star(&target)? },
        StarCommand::Link { larger, smaller } => {
            Request::Link { larger: star(&larger)?, smaller: star(&smaller)? }
        }
        StarCommand::Unlink { larger, smaller } => {
            Request::Unlink { larger: star(&larger)?, smaller: star(&smaller)? }
        }
    };
    universe.handle(request)?;
    Ok(())
}

/// Prints the sessions on a star, and with `all` those on the stars beneath it.
fn show(snapshot: &Snapshot, star: StarId, all: bool) -> Result<()> {
    let view = snapshot.stars.iter().find(|view| view.id == star).context("The star vanished")?;
    let on_star = |key: &SessionKey, stars: &[StarId]| {
        if all { view.sessions.contains(key) } else { stars.contains(&star) }
    };
    let listed = snapshot.sessions.iter().filter(|session| on_star(&session.key, &session.stars));
    print::sessions(snapshot, listed);
    for session in snapshot.missing.iter().filter(|session| on_star(&session.key, &session.stars)) {
        println!("missing  {}", session.key.id);
    }
    Ok(())
}
