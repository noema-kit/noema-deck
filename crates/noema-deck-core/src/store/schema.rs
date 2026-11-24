//! The store's tables, and the steps that bring an older store up to date.

use anyhow::{Result, ensure};
use rusqlite::Connection;

/// Each step takes the store from one schema version to the next.
const MIGRATIONS: [&str; 3] = [
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
    // The records took the names the documents use: domains, parents and assignments.
    "ALTER TABLE star RENAME TO domain;
     ALTER TABLE link RENAME COLUMN larger TO parent;
     ALTER TABLE link RENAME COLUMN smaller TO child;
     DROP INDEX link_smaller;
     CREATE INDEX link_child ON link (child);
     ALTER TABLE placement RENAME TO assignment;
     ALTER TABLE assignment RENAME COLUMN star TO domain;",
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
        connection.execute_batch("PRAGMA user_version = 4")?;

        assert!(Store::prepare(connection).is_err());
        Ok(())
    }

    #[test]
    fn an_older_store_is_upgraded_and_keeps_its_records() -> Result<()> {
        let connection = Connection::open_in_memory()?;
        connection.execute_batch(MIGRATIONS[0])?;
        connection.execute_batch(
            "INSERT INTO star (name) VALUES ('Kept'), ('Under');
             INSERT INTO link (larger, smaller) VALUES (1, 2);
             INSERT INTO placement (star, agent, session) VALUES (2, 'claude', 'a');
             PRAGMA user_version = 1",
        )?;

        let mut store = Store::prepare(connection)?;

        assert_eq!(store.domains()?[0].name, "Kept");
        assert_eq!(store.links()?.len(), 1);
        assert_eq!(store.sessions_on(store.domains()?[0].id, true)?.len(), 1);
        store.set_last_folder(Path::new("/work"))?;
        assert_eq!(store.last_folder()?, Some("/work".into()));
        // Deleting still takes links and assignments along after the renames.
        store.delete_domain(store.domains()?[1].id)?;
        assert!(store.links()?.is_empty() && store.assignments()?.is_empty());
        Ok(())
    }
}
