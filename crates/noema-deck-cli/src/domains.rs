//! Commands about domains.

use anyhow::{Context, Result};
use noema_deck_core::{DomainId, Map, SessionKey, Snapshot, map::Request};

use crate::{DomainCommand, find, print};

pub fn run(map: &mut Map, command: DomainCommand) -> Result<()> {
    let snapshot = map.snapshot()?;
    let domain = |text: &str| find::domain(&snapshot, text);
    let request = match command {
        DomainCommand::List => {
            print::domains(&snapshot);
            return Ok(());
        }
        DomainCommand::Show { domain: target, all } => {
            return show(&snapshot, domain(&target)?, all);
        }
        DomainCommand::Add { name, under } => {
            let parent = under.as_deref().map(domain).transpose()?;
            let id = map.add_domain(&name, parent)?;
            println!("Added domain [{id}] {}", name.trim());
            return Ok(());
        }
        DomainCommand::Import { folder, under, depth } => {
            let parent = under.as_deref().map(domain).transpose()?;
            let added = map.import(&folder, parent, None, depth)?;
            println!("Added {added} domains from {}", folder.display());
            return Ok(());
        }
        DomainCommand::Rename { domain: target, name } => {
            Request::RenameDomain { domain: domain(&target)?, name }
        }
        DomainCommand::Delete { domain: target } => {
            Request::DeleteDomain { domain: domain(&target)? }
        }
        DomainCommand::Link { parent, child } => {
            Request::Link { parent: domain(&parent)?, child: domain(&child)? }
        }
        DomainCommand::Unlink { parent, child } => {
            Request::Unlink { parent: domain(&parent)?, child: domain(&child)? }
        }
    };
    map.handle(request)?;
    Ok(())
}

/// Prints the sessions on a domain, and with `all` those on the subdomains.
fn show(snapshot: &Snapshot, domain: DomainId, all: bool) -> Result<()> {
    let view =
        snapshot.domains.iter().find(|view| view.id == domain).context("The domain vanished")?;
    let on_domain = |key: &SessionKey, domains: &[DomainId]| {
        if all { view.sessions.contains(key) } else { domains.contains(&domain) }
    };
    let listed =
        snapshot.sessions.iter().filter(|session| on_domain(&session.key, &session.domains));
    print::sessions(snapshot, listed, |_| None);
    for session in
        snapshot.missing.iter().filter(|session| on_domain(&session.key, &session.domains))
    {
        println!("missing  {}", session.key.id);
    }
    Ok(())
}
