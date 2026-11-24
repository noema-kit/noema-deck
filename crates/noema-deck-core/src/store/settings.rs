//! Small things Deck remembers between runs.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use rusqlite::OptionalExtension;

use super::Store;

impl Store {
    /// The folder the last new session was started in.
    pub fn last_folder(&self) -> Result<Option<PathBuf>> {
        let folder: Option<String> = self
            .connection
            .query_row("SELECT value FROM setting WHERE name = 'last_folder'", [], |row| row.get(0))
            .optional()?;
        Ok(folder.map(PathBuf::from))
    }

    pub fn set_last_folder(&self, folder: &Path) -> Result<()> {
        let folder = folder.to_str().context("The folder's name is not text")?;
        self.connection.execute(
            "INSERT OR REPLACE INTO setting (name, value) VALUES ('last_folder', ?1)",
            [folder],
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_last_folder_is_remembered_and_replaced() -> Result<()> {
        let store = Store::in_memory()?;
        assert_eq!(store.last_folder()?, None);

        store.set_last_folder(Path::new("/work/a"))?;
        store.set_last_folder(Path::new("/work/b"))?;

        assert_eq!(store.last_folder()?, Some("/work/b".into()));
        Ok(())
    }
}
