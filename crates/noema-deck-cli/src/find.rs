//! Finding the star or session a person named on the command line.

use anyhow::{Result, bail};
use noema_deck_core::{SessionKey, Snapshot, StarId};

/// A star given by its ID, or by a name only one star has.
pub fn star(snapshot: &Snapshot, text: &str) -> Result<StarId> {
    let stars = &snapshot.stars;
    if let Ok(id) = text.parse::<StarId>()
        && stars.iter().any(|star| star.id == id)
    {
        return Ok(id);
    }
    let named: Vec<_> = stars.iter().filter(|star| star.name.eq_ignore_ascii_case(text)).collect();
    match named[..] {
        [star] => Ok(star.id),
        [] => bail!("No star `{text}`"),
        _ => {
            let ids: Vec<_> = named.iter().map(|star| star.id.to_string()).collect();
            bail!("Several stars are named `{text}`. Use an ID: {}", ids.join(", "))
        }
    }
}

/// A session given by its ID, or by a prefix only one of `keys` has.
pub fn session<'a>(
    keys: impl IntoIterator<Item = &'a SessionKey>,
    text: &str,
) -> Result<SessionKey> {
    let found: Vec<_> = keys.into_iter().filter(|key| key.id.starts_with(text)).collect();
    match found[..] {
        [key] => Ok(key.clone()),
        [] => bail!("No session starts with `{text}`"),
        _ => bail!("{} sessions start with `{text}`. Give more of the ID", found.len()),
    }
}
