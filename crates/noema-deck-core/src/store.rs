//! Deck's own records, kept in one `SQLite` file: domains, the links between
//! them, which sessions belong to which domains, and a few settings.

mod assignments;
mod domains;
mod schema;
mod settings;

use std::{
    env, fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};
use rusqlite::Connection;

pub struct Store {
    connection: Connection,
}

impl Store {
    /// Opens the store at `path`, creating it on first use and bringing an
    /// older one up to date.
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).with_context(|| format!("Create {}", parent.display()))?;
        }
        let connection =
            Connection::open(path).with_context(|| format!("Open {}", path.display()))?;
        Self::prepare(connection)
    }

    fn prepare(connection: Connection) -> Result<Self> {
        connection.execute_batch("PRAGMA foreign_keys = ON")?;
        schema::migrate(&connection)?;
        Ok(Self { connection })
    }

    #[cfg(test)]
    pub(crate) fn in_memory() -> Result<Self> {
        Self::prepare(Connection::open_in_memory()?)
    }
}

/// The per-user store: `$NOEMA_DECK_STORE` when set, else
/// `~/Library/Application Support/noema-deck/deck.db` on macOS and
/// `$XDG_DATA_HOME/noema-deck/deck.db` elsewhere.
pub fn default_path() -> Result<PathBuf> {
    let set = |name| env::var_os(name).filter(|value| !value.is_empty()).map(PathBuf::from);
    if let Some(path) = set("NOEMA_DECK_STORE") {
        return Ok(path);
    }
    let home = env::home_dir().context("No home directory")?;
    let data = if cfg!(target_os = "macos") {
        home.join("Library/Application Support")
    } else {
        set("XDG_DATA_HOME").unwrap_or_else(|| home.join(".local/share"))
    };
    Ok(data.join("noema-deck/deck.db"))
}
