//! Small things Deck remembers between runs.

use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, ensure};
use rusqlite::OptionalExtension;
use serde::{Deserialize, Serialize};

use super::Store;
use crate::{agent::Agent, session::SessionKey};

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

/// How Deck looks: its theme, light or dark, and its typefaces.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Appearance {
    /// The theme's folder under `ui/themes`.
    pub theme: String,
    pub mode: Mode,
    /// A typeface for the interface in place of the theme's.
    pub interface_font: Option<String>,
    /// A typeface for terminals in place of the theme's.
    pub terminal_font: Option<String>,
    /// The terminal's text size in pixels.
    pub terminal_size: f64,
    /// The terminal's line height, as a multiple of its text size.
    pub line_height: f64,
}

/// Light or dark, or whichever the system is in. A theme with only one mode
/// keeps to it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    #[default]
    System,
    Light,
    Dark,
}

impl Default for Appearance {
    fn default() -> Self {
        Self {
            theme: "noema".into(),
            mode: Mode::System,
            interface_font: None,
            terminal_font: None,
            terminal_size: 13.0,
            line_height: 1.1,
        }
    }
}

impl Appearance {
    /// The same appearance with blank typefaces left to the theme, or a
    /// refusal of what no theme or terminal could take.
    fn checked(mut self) -> Result<Self> {
        let theme = &self.theme;
        ensure!(
            (1..=40).contains(&theme.len())
                && theme
                    .bytes()
                    .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-'),
            "A theme's name is lowercase letters, digits and dashes"
        );
        for font in [&mut self.interface_font, &mut self.terminal_font] {
            *font =
                font.as_deref().map(str::trim).filter(|name| !name.is_empty()).map(String::from);
            if let Some(name) = font {
                ensure!(
                    name.chars().count() <= 100
                        && !name.chars().any(|c| c.is_control() || "{};".contains(c)),
                    "That is not a typeface's name"
                );
            }
        }
        ensure!((9.0..=24.0).contains(&self.terminal_size), "Text size is from 9 to 24");
        ensure!((1.0..=2.0).contains(&self.line_height), "Line height is from 1 to 2");
        Ok(self)
    }
}

/// The keys the person moved Deck's shortcuts to, by the action each runs,
/// such as `split_right` to `mod+shift+KeyR`. A key is the modifiers it is
/// held with, `mod` (Command, or Control elsewhere), `ctrl`, `alt` and
/// `shift`, then the key's code as the browser names it. An action left out
/// keeps the interface's own key.
pub type Shortcuts = BTreeMap<String, String>;

/// The same shortcuts, or a refusal of a name or key that is not one, or of
/// one key given to two actions.
fn checked_shortcuts(shortcuts: Shortcuts) -> Result<Shortcuts> {
    ensure!(shortcuts.len() <= 64, "Too many shortcuts");
    let mut keys = BTreeSet::new();
    for (action, key) in &shortcuts {
        ensure!(
            (1..=32).contains(&action.len())
                && action.bytes().all(|byte| byte.is_ascii_lowercase() || byte == b'_'),
            "An action's name is lowercase letters and underscores"
        );
        let mut parts: Vec<&str> = key.split('+').collect();
        let code = parts.pop().unwrap_or_default();
        let mut held = BTreeSet::new();
        ensure!(
            (1..=20).contains(&code.len())
                && code.bytes().all(|byte| byte.is_ascii_alphanumeric())
                && parts.iter().all(
                    |part| ["mod", "ctrl", "alt", "shift"].contains(part) && held.insert(*part)
                )
                && (held.contains("mod") || held.contains("ctrl")),
            "A shortcut is held with Command or Control, as `mod+KeyB`"
        );
        ensure!(keys.insert(key.as_str()), "One key cannot run two actions");
    }
    Ok(shortcuts)
}

impl Store {
    /// The folder the last new session was started in.
    pub fn last_folder(&self) -> Result<Option<PathBuf>> {
        Ok(self.setting("last_folder")?.map(PathBuf::from))
    }

    pub fn set_last_folder(&self, folder: &Path) -> Result<()> {
        self.set_setting("last_folder", folder.to_str().context("The folder's name is not text")?)
    }

    /// The agent the last new session was started with.
    pub fn last_agent(&self) -> Result<Agent> {
        Ok(self.setting("last_agent")?.and_then(|name| name.parse().ok()).unwrap_or_default())
    }

    pub fn set_last_agent(&self, agent: Agent) -> Result<()> {
        self.set_setting("last_agent", agent.name())
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

    /// How Deck looks. What an older or newer Deck wrote and this one cannot
    /// read is the default.
    pub fn appearance(&self) -> Result<Appearance> {
        let text = self.setting("appearance")?;
        Ok(text.and_then(|text| serde_json::from_str(&text).ok()).unwrap_or_default())
    }

    pub fn set_appearance(&self, appearance: Appearance) -> Result<()> {
        self.set_setting("appearance", &serde_json::to_string(&appearance.checked()?)?)
    }

    /// The shortcuts moved to other keys. What an older or newer Deck wrote
    /// and this one cannot read is none.
    pub fn shortcuts(&self) -> Result<Shortcuts> {
        let text = self.setting("shortcuts")?;
        Ok(text.and_then(|text| serde_json::from_str(&text).ok()).unwrap_or_default())
    }

    pub fn set_shortcuts(&self, shortcuts: Shortcuts) -> Result<()> {
        self.set_setting("shortcuts", &serde_json::to_string(&checked_shortcuts(shortcuts)?)?)
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

    #[test]
    fn shortcuts_are_kept_and_a_key_that_is_not_one_is_refused() -> Result<()> {
        let store = Store::in_memory()?;
        assert!(store.shortcuts()?.is_empty());
        let moved = Shortcuts::from([
            ("split_right".into(), "mod+shift+KeyR".into()),
            ("fit".into(), "ctrl+alt+Digit9".into()),
        ]);
        store.set_shortcuts(moved.clone())?;
        assert_eq!(store.shortcuts()?, moved);

        let wrong = [
            ("Split".into(), "mod+KeyR".into()),
            ("split".into(), "shift+KeyR".into()),
            ("split".into(), "mod+mod+KeyR".into()),
            ("split".into(), "mod+Key R".into()),
        ];
        for (action, key) in wrong {
            assert!(store.set_shortcuts(Shortcuts::from([(action, key)])).is_err());
        }
        let twice =
            Shortcuts::from([("a".into(), "mod+KeyA".into()), ("b".into(), "mod+KeyA".into())]);
        assert!(store.set_shortcuts(twice).is_err());
        assert_eq!(store.shortcuts()?, moved);
        Ok(())
    }

    #[test]
    fn appearance_is_checked_and_blank_typefaces_go_to_the_theme() -> Result<()> {
        let store = Store::in_memory()?;
        assert_eq!(store.appearance()?, Appearance::default());

        let chosen = Appearance {
            theme: "galaxy".into(),
            mode: Mode::Dark,
            interface_font: Some("  ".into()),
            terminal_font: Some(" JetBrains Mono ".into()),
            terminal_size: 14.0,
            line_height: 1.25,
        };
        store.set_appearance(chosen.clone())?;
        let kept = store.appearance()?;
        assert_eq!(kept.interface_font, None);
        assert_eq!(kept.terminal_font.as_deref(), Some("JetBrains Mono"));
        assert_eq!((kept.theme.as_str(), kept.mode), ("galaxy", Mode::Dark));

        let wrong = [
            Appearance { theme: "../x".into(), ..chosen.clone() },
            Appearance { terminal_font: Some("a; b".into()), ..chosen.clone() },
            Appearance { terminal_size: 40.0, ..chosen.clone() },
            Appearance { line_height: 0.5, ..chosen },
        ];
        for appearance in wrong {
            assert!(store.set_appearance(appearance).is_err());
        }
        assert_eq!(store.appearance()?, kept);
        Ok(())
    }
}
