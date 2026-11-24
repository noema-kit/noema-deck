//! What the commands print.

use std::collections::{HashMap, HashSet};

use chrono::{DateTime, Local};
use noema_deck_core::{Snapshot, StarId, universe::SessionView};

/// Prints one line per session: last activity, ID, stars, folder and title.
pub fn sessions<'a>(snapshot: &Snapshot, shown: impl IntoIterator<Item = &'a SessionView>) {
    let names = names(snapshot);
    let rows: Vec<_> = shown
        .into_iter()
        .map(|session| {
            let updated = DateTime::from_timestamp(session.updated, 0)
                .map(|time| time.with_timezone(&Local).format("%Y-%m-%d %H:%M").to_string())
                .unwrap_or_default();
            let stars: Vec<_> = session.stars.iter().map(|star| names[star]).collect();
            let stars = if stars.is_empty() { "-".into() } else { stars.join(", ") };
            let folder = session.folder.clone().unwrap_or_else(|| "-".into());
            let title: String = session.title.chars().take(80).collect();
            [updated, session.key.id.clone(), stars, folder, title]
        })
        .collect();
    let width =
        |column: usize| rows.iter().map(|row| row[column].chars().count()).max().unwrap_or(0);
    let (stars, folder) = (width(2), width(3));
    for [updated, id, on, at, title] in &rows {
        println!("{updated}  {id}  {on:stars$}  {at:folder$}  {title}");
    }
}

/// Prints each top star with the stars beneath it. A star with several larger
/// stars appears under each of them.
pub fn stars(snapshot: &Snapshot) {
    if snapshot.stars.is_empty() {
        println!("No stars yet. Add one with `noema-deck star add <NAME>`.");
        return;
    }
    let names = names(snapshot);
    let counts: HashMap<_, _> =
        snapshot.stars.iter().map(|star| (star.id, star.sessions.len())).collect();
    let mut beneath = HashMap::<StarId, Vec<StarId>>::new();
    let mut hanging = HashSet::new();
    for &(larger, smaller) in &snapshot.links {
        beneath.entry(larger).or_default().push(smaller);
        hanging.insert(smaller);
    }

    // Stars wait on a stack in reverse name order, so they come off it in order.
    let by_name =
        |stars: &mut Vec<StarId>| stars.sort_by_key(|star| std::cmp::Reverse(names[star]));
    let mut tops: Vec<_> =
        snapshot.stars.iter().map(|star| star.id).filter(|star| !hanging.contains(star)).collect();
    by_name(&mut tops);
    let mut waiting: Vec<_> = tops.into_iter().map(|star| (star, 0)).collect();
    while let Some((star, depth)) = waiting.pop() {
        println!("{:indent$}[{star}] {} ({})", "", names[&star], counts[&star], indent = depth * 2);
        let mut smaller = beneath.get(&star).cloned().unwrap_or_default();
        by_name(&mut smaller);
        waiting.extend(smaller.into_iter().map(|star| (star, depth + 1)));
    }
}

fn names(snapshot: &Snapshot) -> HashMap<StarId, &str> {
    snapshot.stars.iter().map(|star| (star.id, star.name.as_str())).collect()
}
