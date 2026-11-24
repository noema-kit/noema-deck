//! Small things Deck remembers between runs.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use rusqlite::OptionalExtension;
use serde::{Deserialize, Serialize};

use super::Store;
use crate::session::SessionKey;

/// The panes open when Deck last stopped, so the next run can open them again.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct OpenPanes {
    /// In the order they were opened.
    pub open: Vec<OpenPane>,
    /// How the panes on screen were tiled. Absent when none was on screen.
    pub layout: Option<Layout>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum OpenPane {
    Session { session: SessionKey },
    Shell { folder: String, name: Option<String> },
}

/// Panes tiled on screen: one pane, by its place in [`OpenPanes::open`], or
/// several side by side (`row`) or one above another (`column`), each taking
/// its share of the room.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Layout {
    Pane(usize),
    Split { dir: Direction, items: Vec<Layout>, sizes: Vec<f64> },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Direction {
    Row,
    Column,
}

impl Layout {
    /// Whether every pane it names is open, at most once, and every split
    /// holds two or more parts with a positive share each.
    fn fits(&self, open: usize, seen: &mut Vec<usize>) -> bool {
        match self {
            Self::Pane(index) => {
                let fresh = *index < open && !seen.contains(index);
                seen.push(*index);
                fresh
            }
            Self::Split { items, sizes, .. } => {
                items.len() >= 2
                    && sizes.len() == items.len()
                    && sizes.iter().all(|size| size.is_finite() && *size > 0.0)
                    && items.iter().all(|item| item.fits(open, seen))
            }
        }
    }
}

impl Store {
    /// The folder the last new session was started in.
    pub fn last_folder(&self) -> Result<Option<PathBuf>> {
        Ok(self.setting("last_folder")?.map(PathBuf::from))
    }

    pub fn set_last_folder(&self, folder: &Path) -> Result<()> {
        self.set_setting("last_folder", folder.to_str().context("The folder's name is not text")?)
    }

    /// The panes open when Deck last stopped. What an older or newer Deck
    /// wrote and this one cannot read is dropped.
    pub fn open_panes(&self) -> Result<OpenPanes> {
        let text = self.setting("open_panes")?;
        Ok(text.and_then(|text| serde_json::from_str(&text).ok()).unwrap_or_default())
    }

    /// Keeps `panes`, leaving out a layout that names a pane not open.
    pub fn set_open_panes(&self, mut panes: OpenPanes) -> Result<()> {
        let fits = panes
            .layout
            .as_ref()
            .is_none_or(|layout| layout.fits(panes.open.len(), &mut Vec::new()));
        if !fits {
            panes.layout = None;
        }
        self.set_setting("open_panes", &serde_json::to_string(&panes)?)
    }

    fn setting(&self, name: &str) -> Result<Option<String>> {
        let value = self
            .connection
            .query_row("SELECT value FROM setting WHERE name = ?1", [name], |row| row.get(0))
            .optional()?;
        Ok(value)
    }

    fn set_setting(&self, name: &str, value: &str) -> Result<()> {
        self.connection.execute(
            "INSERT OR REPLACE INTO setting (name, value) VALUES (?1, ?2)",
            [name, value],
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::Agent;

    #[test]
    fn the_last_folder_is_remembered_and_reassigned() -> Result<()> {
        let store = Store::in_memory()?;
        assert_eq!(store.last_folder()?, None);

        store.set_last_folder(Path::new("/work/a"))?;
        store.set_last_folder(Path::new("/work/b"))?;

        assert_eq!(store.last_folder()?, Some("/work/b".into()));
        Ok(())
    }

    #[test]
    fn open_panes_are_remembered_and_unreadable_ones_forgotten() -> Result<()> {
        let store = Store::in_memory()?;
        assert_eq!(store.open_panes()?, OpenPanes::default());
        let session = SessionKey { agent: Agent::Claude, id: "a".into() };
        let open = vec![
            OpenPane::Session { session },
            OpenPane::Shell { folder: "~/work".into(), name: Some("Build".into()) },
            OpenPane::Shell { folder: "~".into(), name: None },
        ];
        let below = Layout::Split {
            dir: Direction::Column,
            items: vec![Layout::Pane(1), Layout::Pane(2)],
            sizes: vec![0.5, 0.5],
        };
        let layout = Layout::Split {
            dir: Direction::Row,
            items: vec![Layout::Pane(0), below],
            sizes: vec![0.6, 0.4],
        };
        let panes = OpenPanes { open, layout: Some(layout) };

        store.set_open_panes(panes.clone())?;
        assert_eq!(store.open_panes()?, panes);

        let twice = Layout::Split {
            dir: Direction::Row,
            items: vec![Layout::Pane(0), Layout::Pane(0)],
            sizes: vec![1.0, 1.0],
        };
        for wrong in [Layout::Pane(3), twice] {
            store.set_open_panes(OpenPanes { layout: Some(wrong), ..panes.clone() })?;
            assert_eq!(store.open_panes()?.layout, None);
        }

        store.set_setting("open_panes", r#"{"open":[{"kind":"window"}]}"#)?;
        assert_eq!(store.open_panes()?, OpenPanes::default());
        Ok(())
    }
}
