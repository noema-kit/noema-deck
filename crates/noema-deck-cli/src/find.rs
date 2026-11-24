//! Finding the domain or session a person named on the command line.

use anyhow::{Result, bail};
use noema_deck_core::{DomainId, SessionKey, Snapshot};

/// A domain given by its ID, or by a name only one domain has.
pub fn domain(snapshot: &Snapshot, text: &str) -> Result<DomainId> {
    let domains = &snapshot.domains;
    if let Ok(id) = text.parse::<DomainId>()
        && domains.iter().any(|domain| domain.id == id)
    {
        return Ok(id);
    }
    let named: Vec<_> =
        domains.iter().filter(|domain| domain.name.eq_ignore_ascii_case(text)).collect();
    match named[..] {
        [domain] => Ok(domain.id),
        [] => bail!("No domain `{text}`"),
        _ => {
            let ids: Vec<_> = named.iter().map(|domain| domain.id.to_string()).collect();
            bail!("Several domains are named `{text}`. Use an ID: {}", ids.join(", "))
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
