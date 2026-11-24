//! What the commands print.

use std::collections::{HashMap, HashSet};

use chrono::{DateTime, Local};
use noema_deck_core::{DomainId, Snapshot, map::SessionView};

/// Prints one line per session: last activity, ID, domains, folder and title.
pub fn sessions<'a>(snapshot: &Snapshot, shown: impl IntoIterator<Item = &'a SessionView>) {
    let names = names(snapshot);
    let rows: Vec<_> = shown
        .into_iter()
        .map(|session| {
            let updated = DateTime::from_timestamp(session.updated, 0)
                .map(|time| time.with_timezone(&Local).format("%Y-%m-%d %H:%M").to_string())
                .unwrap_or_default();
            let domains: Vec<_> = session.domains.iter().map(|domain| names[domain]).collect();
            let domains = if domains.is_empty() { "-".into() } else { domains.join(", ") };
            let folder = session.folder.clone().unwrap_or_else(|| "-".into());
            let title: String = session.title.chars().take(80).collect();
            [updated, session.key.id.clone(), domains, folder, title]
        })
        .collect();
    let width =
        |column: usize| rows.iter().map(|row| row[column].chars().count()).max().unwrap_or(0);
    let (domains, folder) = (width(2), width(3));
    for [updated, id, on, at, title] in &rows {
        println!("{updated}  {id}  {on:domains$}  {at:folder$}  {title}");
    }
}

/// Prints each root domain with its subdomains. A domain with several parent
/// domains appears under each of them.
pub fn domains(snapshot: &Snapshot) {
    if snapshot.domains.is_empty() {
        println!("No domains yet. Add one with `noema-deck domain add <NAME>`.");
        return;
    }
    let names = names(snapshot);
    let counts: HashMap<_, _> =
        snapshot.domains.iter().map(|domain| (domain.id, domain.sessions.len())).collect();
    let mut under = HashMap::<DomainId, Vec<DomainId>>::new();
    let mut hanging = HashSet::new();
    for &(parent, child) in &snapshot.links {
        under.entry(parent).or_default().push(child);
        hanging.insert(child);
    }

    // Domains wait on a stack in reverse name order, so they come off it in order.
    let by_name = |domains: &mut Vec<DomainId>| {
        domains.sort_by_key(|domain| std::cmp::Reverse(names[domain]));
    };
    let mut tops: Vec<_> = snapshot
        .domains
        .iter()
        .map(|domain| domain.id)
        .filter(|domain| !hanging.contains(domain))
        .collect();
    by_name(&mut tops);
    let mut waiting: Vec<_> = tops.into_iter().map(|domain| (domain, 0)).collect();
    while let Some((domain, depth)) = waiting.pop() {
        println!(
            "{:indent$}[{domain}] {} ({})",
            "",
            names[&domain],
            counts[&domain],
            indent = depth * 2
        );
        let mut child = under.get(&domain).cloned().unwrap_or_default();
        by_name(&mut child);
        waiting.extend(child.into_iter().map(|domain| (domain, depth + 1)));
    }
}

fn names(snapshot: &Snapshot) -> HashMap<DomainId, &str> {
    snapshot.domains.iter().map(|domain| (domain.id, domain.name.as_str())).collect()
}
