//! The store's tables, and the steps that bring an older store up to date.

use anyhow::{Result, ensure};
use rusqlite::Connection;

/// Each step takes the store from one schema version to the next.
const MIGRATIONS: [&str; 2] = [
    "CREATE TABLE star (
         id   INTEGER PRIMARY KEY AUTOINCREMENT,
         name TEXT NOT NULL,
         x    REAL NOT NULL DEFAULT 0,
         y    REAL NOT NULL DEFAULT 0
     );
     CREATE TABLE link (
         larger  INTEGER NOT NULL REFERENCES star (id) ON DELETE CASCADE,
         smaller INTEGER NOT NULL REFERENCES star (id) ON DELETE CASCADE,
         PRIMARY KEY (larger, smaller),
         CHECK (larger <> smaller)
     ) WITHOUT ROWID;
     CREATE INDEX link_smaller ON link (smaller);
     CREATE TABLE placement (
         star    INTEGER NOT NULL REFERENCES star (id) ON DELETE CASCADE,
         agent   TEXT NOT NULL,
         session TEXT NOT NULL,
         PRIMARY KEY (star, agent, session)
     ) WITHOUT ROWID;",
    "CREATE TABLE setting (name TEXT PRIMARY KEY, value TEXT NOT NULL) WITHOUT ROWID;",
];

/// Runs every step the store has not yet had. A store from a newer Deck is
/// refused, since its tables may mean something this version cannot know.
pub(super) fn migrate(connection: &Connection) -> Result<()> {
    let version: i64 = connection.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    let done = usize::try_from(version).unwrap_or(usize::MAX);
    ensure!(done <= MIGRATIONS.len(), "The store was written by a newer Deck (schema {version})");
    for (index, step) in MIGRATIONS.iter().enumerate().skip(done) {
        let next = index + 1;
        connection
            .execute_batch(&format!("BEGIN; {step} PRAGMA user_version = {next}; COMMIT;"))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;
    use crate::store::Store;

    #[test]
    fn a_store_from_a_newer_deck_is_refused() -> Result<()> {
        let connection = Connection::open_in_memory()?;
        connection.execute_batch("PRAGMA user_version = 3")?;

        assert!(Store::prepare(connection).is_err());
        Ok(())
    }

    #[test]
    fn an_older_store_gains_what_is_new_and_keeps_its_stars() -> Result<()> {
        let connection = Connection::open_in_memory()?;
        connection.execute_batch(MIGRATIONS[0])?;
        connection
            .execute_batch("INSERT INTO star (name) VALUES ('Kept'); PRAGMA user_version = 1")?;

        let store = Store::prepare(connection)?;

        assert_eq!(store.stars()?[0].name, "Kept");
        assert_eq!(store.last_folder()?, None);
        store.set_last_folder(Path::new("/work"))?;
        assert_eq!(store.last_folder()?, Some("/work".into()));
        Ok(())
    }
}
