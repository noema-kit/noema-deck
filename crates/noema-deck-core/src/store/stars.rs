//! Stars and the links between them.

use anyhow::{Context, Result, ensure};

use super::Store;
use crate::{
    import::Seed,
    star::{Star, StarId},
};

impl Store {
    /// Adds a star under `larger`, or a top star when there is none. Without a
    /// `position` it appears beside its larger star, or in a row of top stars.
    pub fn add_star(
        &mut self,
        name: &str,
        larger: Option<StarId>,
        position: Option<(f64, f64)>,
    ) -> Result<StarId> {
        let (x, y) = match position {
            Some(position) => position,
            None => self.free_spot(larger)?,
        };
        let seed = Seed { name: name.into(), larger: None, x, y };
        let stars = self.add_stars(&[seed], larger)?;
        stars.first().copied().context("The star was not added")
    }

    /// Adds every seed in one step, so a tree arrives whole or not at all. A
    /// seed with no larger seed hangs under `larger`, or is a top star.
    pub fn add_stars(&mut self, seeds: &[Seed], larger: Option<StarId>) -> Result<Vec<StarId>> {
        if let Some(larger) = larger {
            self.require(larger)?;
        }
        let transaction = self.connection.transaction()?;
        let mut stars: Vec<StarId> = Vec::with_capacity(seeds.len());
        for seed in seeds {
            let name = valid_name(&seed.name)?;
            transaction.execute(
                "INSERT INTO star (name, x, y) VALUES (?1, ?2, ?3)",
                (name, seed.x, seed.y),
            )?;
            let star = StarId::new(transaction.last_insert_rowid());
            let above = match seed.larger {
                Some(index) => Some(*stars.get(index).context("A seed hangs under a later one")?),
                None => larger,
            };
            if let Some(above) = above {
                transaction
                    .execute("INSERT INTO link (larger, smaller) VALUES (?1, ?2)", (above, star))?;
            }
            stars.push(star);
        }
        transaction.commit()?;
        Ok(stars)
    }

    /// Where a new star goes when nobody chose: beside its larger star, or at
    /// the end of the row of top stars.
    pub fn free_spot(&self, larger: Option<StarId>) -> Result<(f64, f64)> {
        let Some(larger) = larger else {
            let tops: f64 = self.connection.query_row(
                "SELECT COUNT(*) FROM star WHERE id NOT IN (SELECT smaller FROM link)",
                [],
                |row| row.get(0),
            )?;
            return Ok((tops * 420.0, 0.0));
        };
        self.require(larger)?;
        let (x, y, smaller): (f64, f64, f64) = self.connection.query_row(
            "SELECT x, y, (SELECT COUNT(*) FROM link WHERE larger = ?1) FROM star WHERE id = ?1",
            [larger],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )?;
        // The golden angle keeps successive stars apart however many there are.
        let angle = smaller * 2.4;
        Ok((x + 170.0 * angle.cos(), y + 170.0 * angle.sin()))
    }

    pub fn rename_star(&self, star: StarId, name: &str) -> Result<()> {
        let name = valid_name(name)?;
        let changed =
            self.connection.execute("UPDATE star SET name = ?2 WHERE id = ?1", (star, name))?;
        ensure!(changed == 1, "No star {star}");
        Ok(())
    }

    pub fn move_star(&self, star: StarId, x: f64, y: f64) -> Result<()> {
        let changed = self
            .connection
            .execute("UPDATE star SET x = ?2, y = ?3 WHERE id = ?1", (star, x, y))?;
        ensure!(changed == 1, "No star {star}");
        Ok(())
    }

    /// Deletes a star with its links and placements. A smaller star left with
    /// no larger star moves up to the deleted star's larger stars, or becomes
    /// a top star when there are none.
    pub fn delete_star(&mut self, star: StarId) -> Result<()> {
        self.require(star)?;
        let transaction = self.connection.transaction()?;
        transaction.execute(
            "INSERT INTO link (larger, smaller)
             SELECT up.larger, down.smaller FROM link AS up, link AS down
             WHERE up.smaller = ?1 AND down.larger = ?1 AND NOT EXISTS (
                 SELECT 1 FROM link WHERE smaller = down.smaller AND larger <> ?1)",
            [star],
        )?;
        transaction.execute("DELETE FROM star WHERE id = ?1", [star])?;
        transaction.commit()?;
        Ok(())
    }

    /// Hangs `smaller` under `larger`. Refuses a link that would place a star
    /// under itself.
    pub fn link(&self, larger: StarId, smaller: StarId) -> Result<()> {
        let (above, below) = (self.name(larger)?, self.name(smaller)?);
        ensure!(larger != smaller, "{above} cannot hang under itself");
        ensure!(
            !self.is_under(larger, smaller)?,
            "{above} already hangs under {below}, so this link would form a loop"
        );
        self.connection.execute(
            "INSERT OR IGNORE INTO link (larger, smaller) VALUES (?1, ?2)",
            (larger, smaller),
        )?;
        Ok(())
    }

    /// Removes a link. A star that loses its last link becomes a top star.
    pub fn unlink(&self, larger: StarId, smaller: StarId) -> Result<()> {
        let changed = self
            .connection
            .execute("DELETE FROM link WHERE larger = ?1 AND smaller = ?2", (larger, smaller))?;
        ensure!(changed == 1, "Star {smaller} does not hang under star {larger}");
        Ok(())
    }

    pub fn stars(&self) -> Result<Vec<Star>> {
        let mut statement =
            self.connection.prepare("SELECT id, name, x, y FROM star ORDER BY id")?;
        let stars = statement
            .query_map([], |row| {
                Ok(Star { id: row.get(0)?, name: row.get(1)?, x: row.get(2)?, y: row.get(3)? })
            })?
            .collect::<rusqlite::Result<_>>()?;
        Ok(stars)
    }

    /// Every link as `(larger, smaller)`.
    pub fn links(&self) -> Result<Vec<(StarId, StarId)>> {
        let mut statement =
            self.connection.prepare("SELECT larger, smaller FROM link ORDER BY larger, smaller")?;
        let links = statement
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
            .collect::<rusqlite::Result<_>>()?;
        Ok(links)
    }

    /// Fails unless the star exists.
    pub fn require(&self, star: StarId) -> Result<()> {
        self.name(star).map(drop)
    }

    fn name(&self, star: StarId) -> Result<String> {
        let mut statement = self.connection.prepare("SELECT name FROM star WHERE id = ?1")?;
        let name = statement.query_map([star], |row| row.get(0))?.next().transpose()?;
        name.with_context(|| format!("No star {star}"))
    }

    /// Whether `star` hangs under `ancestor`, directly or through other stars.
    fn is_under(&self, star: StarId, ancestor: StarId) -> Result<bool> {
        let found = self.connection.query_row(
            "WITH RECURSIVE under (id) AS (
                 SELECT smaller FROM link WHERE larger = ?2
                 UNION
                 SELECT link.smaller FROM link JOIN under ON link.larger = under.id)
             SELECT EXISTS (SELECT 1 FROM under WHERE id = ?1)",
            (star, ancestor),
            |row| row.get(0),
        )?;
        Ok(found)
    }
}

fn valid_name(name: &str) -> Result<&str> {
    let name = name.trim();
    ensure!(!name.is_empty(), "A star needs a name");
    Ok(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    const MISSING: StarId = StarId::new(99);

    #[test]
    fn a_star_can_hang_under_several_larger_stars() -> Result<()> {
        let mut store = Store::in_memory()?;
        let system = store.add_star("System", None, None)?;
        let memory = store.add_star(" Memory ", Some(system), None)?;
        let distributed = store.add_star("Distributed system", Some(system), None)?;
        let cxl = store.add_star("CXL", Some(memory), None)?;
        store.link(distributed, cxl)?;
        store.link(distributed, cxl)?;

        let links = [(system, memory), (system, distributed), (memory, cxl), (distributed, cxl)];
        assert_eq!(store.links()?, links);
        assert_eq!(store.stars()?[1].name, "Memory");
        assert!(store.add_star("  ", None, None).is_err());
        assert!(store.add_star("Orphan", Some(MISSING), None).is_err());
        assert!(store.add_star("Lost", Some(MISSING), Some((0.0, 0.0))).is_err());
        Ok(())
    }

    #[test]
    fn a_link_that_would_form_a_loop_is_refused() -> Result<()> {
        let mut store = Store::in_memory()?;
        let top = store.add_star("Top", None, None)?;
        let middle = store.add_star("Middle", Some(top), None)?;
        let bottom = store.add_star("Bottom", Some(middle), None)?;

        assert!(store.link(top, top).is_err());
        assert!(store.link(middle, top).is_err());
        assert!(store.link(bottom, top).is_err());
        assert!(store.link(top, MISSING).is_err());
        store.link(top, bottom)?;
        Ok(())
    }

    #[test]
    fn removing_the_last_link_makes_a_top_star() -> Result<()> {
        let mut store = Store::in_memory()?;
        let top = store.add_star("Top", None, None)?;
        let smaller = store.add_star("Smaller", Some(top), None)?;

        store.unlink(top, smaller)?;

        assert!(store.links()?.is_empty());
        assert_eq!(store.stars()?.len(), 2);
        assert!(store.unlink(top, smaller).is_err());
        Ok(())
    }

    #[test]
    fn deleting_a_star_moves_stranded_smaller_stars_up() -> Result<()> {
        let mut store = Store::in_memory()?;
        let top = store.add_star("Top", None, None)?;
        let other = store.add_star("Other", None, None)?;
        let middle = store.add_star("Middle", Some(top), None)?;
        let stranded = store.add_star("Stranded", Some(middle), None)?;
        let shared = store.add_star("Shared", Some(middle), None)?;
        store.link(other, shared)?;

        store.delete_star(middle)?;

        assert_eq!(store.links()?, [(top, stranded), (other, shared)]);
        assert!(store.delete_star(middle).is_err());

        store.delete_star(top)?;
        assert_eq!(store.links()?, [(other, shared)]);
        assert_eq!(store.stars()?.len(), 3);
        Ok(())
    }

    #[test]
    fn a_new_star_appears_beside_its_larger_star() -> Result<()> {
        let mut store = Store::in_memory()?;
        let first = store.add_star("First", None, None)?;
        let second = store.add_star("Second", None, None)?;
        let placed = store.add_star("Placed", Some(first), Some((5.0, 6.0)))?;
        let near = store.add_star("Near", Some(second), None)?;
        let next = store.add_star("Next", Some(second), None)?;

        let stars = store.stars()?;
        let at = |id| stars.iter().find(|star| star.id == id).map(|star| (star.x, star.y));
        assert_eq!(at(first), Some((0.0, 0.0)));
        assert_eq!(at(second), Some((420.0, 0.0)));
        assert_eq!(at(placed), Some((5.0, 6.0)));
        let (near, next) = (at(near).context("near")?, at(next).context("next")?);
        assert!(((near.0 - 420.0).hypot(near.1) - 170.0).abs() < 1e-9);
        assert!((near.0 - next.0).hypot(near.1 - next.1) > 100.0);
        Ok(())
    }

    #[test]
    fn a_tree_of_seeds_arrives_whole_or_not_at_all() -> Result<()> {
        let mut store = Store::in_memory()?;
        let top = store.add_star("Top", None, None)?;
        let seed = |name: &str, larger, x| Seed { name: name.into(), larger, x, y: 2.0 };
        let seeds = [seed("Root", None, 1.0), seed("Leaf", Some(0), 3.0)];

        let stars = store.add_stars(&seeds, Some(top))?;

        assert_eq!(store.links()?, [(top, stars[0]), (stars[0], stars[1])]);
        assert_eq!((store.stars()?[2].x, store.stars()?[2].y), (3.0, 2.0));
        assert!(store.add_stars(&seeds, Some(MISSING)).is_err());
        let unnamed = [seed("Fine", None, 0.0), seed(" ", Some(0), 0.0)];
        assert!(store.add_stars(&unnamed, None).is_err());
        assert_eq!(store.stars()?.len(), 3);
        Ok(())
    }

    #[test]
    fn stars_keep_names_and_positions_across_reopening() -> Result<()> {
        let folder = tempfile::tempdir()?;
        let path = folder.path().join("nested/deck.db");
        let mut store = Store::open(&path)?;
        let star = store.add_star("System", None, None)?;
        store.rename_star(star, "Systems")?;
        store.move_star(star, 12.5, -3.0)?;
        assert!(store.rename_star(star, "").is_err());
        assert!(store.move_star(MISSING, 0.0, 0.0).is_err());
        drop(store);

        let store = Store::open(&path)?;

        let kept = Star { id: star, name: "Systems".into(), x: 12.5, y: -3.0 };
        assert_eq!(store.stars()?, [kept]);
        Ok(())
    }
}
