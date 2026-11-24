//! Turning a folder tree into stars: a folder becomes a star, and each folder
//! inside it a smaller star beneath.

use std::{f64::consts::TAU, fs, path::Path};

use anyhow::{Context, Result, ensure};

/// How many levels of folders inside the imported one become stars by default.
pub const DEPTH: usize = 2;
/// A tree with more folders than this is refused as a likely mistake.
const LIMIT: usize = 300;
/// The room each outermost star gets along its ring.
const GAP: f64 = 120.0;
/// The least distance between one ring and the next.
const RING: f64 = 230.0;

/// A star to add: its name, its place, and the earlier seed it hangs under.
#[derive(Clone, Debug, PartialEq)]
pub struct Seed {
    pub name: String,
    pub larger: Option<usize>,
    pub x: f64,
    pub y: f64,
}

struct Folder {
    name: String,
    inside: Vec<Folder>,
}

impl Folder {
    fn count(&self) -> usize {
        1 + self.inside.iter().map(Self::count).sum::<usize>()
    }

    /// The outermost folders beneath, which is the share of a ring this
    /// folder's stars need.
    fn span(&self) -> usize {
        self.inside.iter().map(Self::span).sum::<usize>().max(1)
    }

    fn levels(&self) -> usize {
        self.inside.iter().map(|folder| 1 + folder.levels()).max().unwrap_or(0)
    }
}

/// The stars for `folder` and the folders up to `depth` levels inside it. The
/// folder's own star sits at `origin` and each level forms a ring around it.
/// Hidden folders are left out and links are not followed.
pub fn seeds(folder: &Path, depth: usize, origin: (f64, f64)) -> Result<Vec<Seed>> {
    let folder =
        folder.canonicalize().with_context(|| format!("{} is not a folder", folder.display()))?;
    fs::read_dir(&folder).with_context(|| format!("{} is not a folder", folder.display()))?;
    let tree = read(&folder, depth)
        .with_context(|| format!("{} has no name to give a star", folder.display()))?;
    let count = tree.count();
    ensure!(
        count <= LIMIT,
        "{} holds {count} folders within {depth} levels, and one import takes at most {LIMIT}",
        folder.display()
    );

    // The outermost ring has to fit every outermost folder side by side.
    let step = (tree.span() as f64 * GAP / (TAU * tree.levels().max(1) as f64)).max(RING);
    let mut layout = Layout { origin, step, seeds: Vec::with_capacity(count) };
    layout.plant(&tree, None, 0, (-TAU / 4.0, TAU));
    Ok(layout.seeds)
}

/// Reads `folder` and what is inside it. `None` when its name is not text.
fn read(folder: &Path, depth: usize) -> Option<Folder> {
    let name = folder.file_name()?.to_str()?.to_owned();
    let mut inside = Vec::new();
    // A folder that cannot be read still becomes a star, with none beneath it.
    if let (1.., Ok(entries)) = (depth, fs::read_dir(folder)) {
        for entry in entries.flatten() {
            let hidden = entry.file_name().to_string_lossy().starts_with('.');
            // The entry's own type is asked, so a link to a folder is skipped
            // and a tree cannot lead back into itself.
            if !hidden && entry.file_type().is_ok_and(|kind| kind.is_dir()) {
                inside.extend(read(&entry.path(), depth - 1));
            }
        }
        inside.sort_by(|a, b| a.name.cmp(&b.name));
    }
    Some(Folder { name, inside })
}

struct Layout {
    origin: (f64, f64),
    /// How far each ring lies beyond the one inside it.
    step: f64,
    seeds: Vec<Seed>,
}

impl Layout {
    /// Places `folder` in the middle of its `arc`, a start angle and a width,
    /// on the ring for its `level`, then shares the arc among what is inside.
    fn plant(&mut self, folder: &Folder, larger: Option<usize>, level: usize, arc: (f64, f64)) {
        let (start, width) = arc;
        let (angle, radius) = (start + width / 2.0, level as f64 * self.step);
        let index = self.seeds.len();
        self.seeds.push(Seed {
            name: folder.name.clone(),
            larger,
            x: self.origin.0 + radius * angle.cos(),
            y: self.origin.1 + radius * angle.sin(),
        });
        let mut start = start;
        for inner in &folder.inside {
            let share = width * inner.span() as f64 / folder.span() as f64;
            self.plant(inner, Some(index), level + 1, (start, share));
            start += share;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tree(folders: &[&str]) -> Result<tempfile::TempDir> {
        let root = tempfile::tempdir()?;
        for folder in folders {
            fs::create_dir_all(root.path().join(folder))?;
        }
        Ok(root)
    }

    #[test]
    fn a_nested_folder_becomes_a_star_under_its_parent_folders_star() -> Result<()> {
        let root = tree(&["topics/b/x", "topics/b/y/too-deep", "topics/a", "topics/.git/objects"])?;
        fs::write(root.path().join("topics/notes.txt"), "not a folder")?;

        let seeds = seeds(&root.path().join("topics"), 2, (100.0, 50.0))?;

        let found: Vec<_> = seeds.iter().map(|seed| (seed.name.as_str(), seed.larger)).collect();
        let expected =
            [("topics", None), ("a", Some(0)), ("b", Some(0)), ("x", Some(2)), ("y", Some(2))];
        assert_eq!(found, expected);
        Ok(())
    }

    #[test]
    fn each_level_forms_a_ring_with_room_between_stars() -> Result<()> {
        let root = tree(&["t/a/1", "t/a/2", "t/a/3", "t/b/4", "t/c", "t/d/5", "t/d/6"])?;

        let seeds = seeds(&root.path().join("t"), 2, (100.0, 50.0))?;

        let from_origin = |seed: &Seed| (seed.x - 100.0).hypot(seed.y - 50.0);
        assert_eq!((seeds[0].x, seeds[0].y), (100.0, 50.0));
        for seed in &seeds[1..] {
            let rings = if seed.larger == Some(0) { 1.0 } else { 2.0 };
            assert!((from_origin(seed) - rings * RING).abs() < 1e-6, "{seed:?}");
        }
        for (index, seed) in seeds.iter().enumerate() {
            for other in &seeds[index + 1..] {
                assert!((seed.x - other.x).hypot(seed.y - other.y) > 100.0, "{seed:?} {other:?}");
            }
        }
        Ok(())
    }

    #[test]
    fn depth_bounds_the_tree_and_a_missing_folder_is_refused() -> Result<()> {
        let root = tree(&["t/a/b"])?;

        assert_eq!(seeds(&root.path().join("t"), 0, (0.0, 0.0))?.len(), 1);
        assert_eq!(seeds(&root.path().join("t"), 1, (0.0, 0.0))?.len(), 2);
        assert!(seeds(&root.path().join("gone"), 2, (0.0, 0.0)).is_err());
        Ok(())
    }

    #[test]
    fn a_tree_too_large_to_be_meant_is_refused() -> Result<()> {
        let folders: Vec<_> = (0..LIMIT).map(|number| format!("t/{number}")).collect();
        let root = tree(&folders.iter().map(String::as_str).collect::<Vec<_>>())?;

        let refused = seeds(&root.path().join("t"), 1, (0.0, 0.0));

        assert!(refused.is_err_and(|error| error.to_string().contains("at most")));
        Ok(())
    }
}
