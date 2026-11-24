//! Domains and the links between them.

use anyhow::{Context, Result, ensure};

use super::Store;
use crate::{
    domain::{Domain, DomainId},
    import::Seed,
};

impl Store {
    /// Adds a domain under `parent`, or a root domain when there is none. Without a
    /// `position` it appears beside its parent domain, or in a row of root domains.
    pub fn add_domain(
        &mut self,
        name: &str,
        parent: Option<DomainId>,
        position: Option<(f64, f64)>,
    ) -> Result<DomainId> {
        let (x, y) = match position {
            Some(position) => position,
            None => self.free_spot(parent)?,
        };
        let seed = Seed { name: name.into(), parent: None, x, y };
        let domains = self.add_domains(&[seed], parent)?;
        domains.first().copied().context("The domain was not added")
    }

    /// Adds every seed in one step, so a tree arrives whole or not at all. A
    /// seed with no parent seed is under `parent`, or is a root domain.
    pub fn add_domains(
        &mut self,
        seeds: &[Seed],
        parent: Option<DomainId>,
    ) -> Result<Vec<DomainId>> {
        if let Some(parent) = parent {
            self.require(parent)?;
        }
        let transaction = self.connection.transaction()?;
        let mut domains: Vec<DomainId> = Vec::with_capacity(seeds.len());
        for seed in seeds {
            let name = valid_name(&seed.name)?;
            transaction.execute(
                "INSERT INTO domain (name, x, y) VALUES (?1, ?2, ?3)",
                (name, seed.x, seed.y),
            )?;
            let domain = DomainId::new(transaction.last_insert_rowid());
            let above = match seed.parent {
                Some(index) => Some(*domains.get(index).context("A seed is under a later one")?),
                None => parent,
            };
            if let Some(above) = above {
                transaction
                    .execute("INSERT INTO link (parent, child) VALUES (?1, ?2)", (above, domain))?;
            }
            domains.push(domain);
        }
        transaction.commit()?;
        Ok(domains)
    }

    /// Where a new domain goes when nobody chose: beside its parent domain, or at
    /// the end of the row of root domains.
    pub fn free_spot(&self, parent: Option<DomainId>) -> Result<(f64, f64)> {
        let Some(parent) = parent else {
            let tops: f64 = self.connection.query_row(
                "SELECT COUNT(*) FROM domain WHERE id NOT IN (SELECT child FROM link)",
                [],
                |row| row.get(0),
            )?;
            return Ok((tops * 420.0, 0.0));
        };
        self.require(parent)?;
        let (x, y, child): (f64, f64, f64) = self.connection.query_row(
            "SELECT x, y, (SELECT COUNT(*) FROM link WHERE parent = ?1) FROM domain WHERE id = ?1",
            [parent],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )?;
        // The golden angle keeps successive domains apart however many there are.
        let angle = child * 2.4;
        Ok((x + 170.0 * angle.cos(), y + 170.0 * angle.sin()))
    }

    pub fn rename_domain(&self, domain: DomainId, name: &str) -> Result<()> {
        let name = valid_name(name)?;
        let changed =
            self.connection.execute("UPDATE domain SET name = ?2 WHERE id = ?1", (domain, name))?;
        ensure!(changed == 1, "No domain {domain}");
        Ok(())
    }

    pub fn move_domain(&self, domain: DomainId, x: f64, y: f64) -> Result<()> {
        let changed = self
            .connection
            .execute("UPDATE domain SET x = ?2, y = ?3 WHERE id = ?1", (domain, x, y))?;
        ensure!(changed == 1, "No domain {domain}");
        Ok(())
    }

    /// Deletes a domain with its links and assignments. A subdomain left with
    /// no parent domain moves up to the deleted domain's parent domains, or becomes
    /// a root domain when there are none.
    pub fn delete_domain(&mut self, domain: DomainId) -> Result<()> {
        self.require(domain)?;
        let transaction = self.connection.transaction()?;
        transaction.execute(
            "INSERT INTO link (parent, child)
             SELECT up.parent, down.child FROM link AS up, link AS down
             WHERE up.child = ?1 AND down.parent = ?1 AND NOT EXISTS (
                 SELECT 1 FROM link WHERE child = down.child AND parent <> ?1)",
            [domain],
        )?;
        transaction.execute("DELETE FROM domain WHERE id = ?1", [domain])?;
        transaction.commit()?;
        Ok(())
    }

    /// Makes `child` a subdomain of `parent`. Refuses a link that would put a domain
    /// under itself.
    pub fn link(&self, parent: DomainId, child: DomainId) -> Result<()> {
        let (above, below) = (self.name(parent)?, self.name(child)?);
        ensure!(parent != child, "{above} cannot be its own parent");
        ensure!(
            !self.is_under(parent, child)?,
            "{above} is already under {below}, so this link would form a loop"
        );
        self.connection.execute(
            "INSERT OR IGNORE INTO link (parent, child) VALUES (?1, ?2)",
            (parent, child),
        )?;
        Ok(())
    }

    /// Removes a link. A domain that loses its last link becomes a root domain.
    pub fn unlink(&self, parent: DomainId, child: DomainId) -> Result<()> {
        let changed = self
            .connection
            .execute("DELETE FROM link WHERE parent = ?1 AND child = ?2", (parent, child))?;
        ensure!(changed == 1, "Domain {child} is not under domain {parent}");
        Ok(())
    }

    pub fn domains(&self) -> Result<Vec<Domain>> {
        let mut statement =
            self.connection.prepare("SELECT id, name, x, y FROM domain ORDER BY id")?;
        let domains = statement
            .query_map([], |row| {
                Ok(Domain { id: row.get(0)?, name: row.get(1)?, x: row.get(2)?, y: row.get(3)? })
            })?
            .collect::<rusqlite::Result<_>>()?;
        Ok(domains)
    }

    /// Every link as `(parent, child)`.
    pub fn links(&self) -> Result<Vec<(DomainId, DomainId)>> {
        let mut statement =
            self.connection.prepare("SELECT parent, child FROM link ORDER BY parent, child")?;
        let links = statement
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
            .collect::<rusqlite::Result<_>>()?;
        Ok(links)
    }

    /// Fails unless the domain exists.
    pub fn require(&self, domain: DomainId) -> Result<()> {
        self.name(domain).map(drop)
    }

    fn name(&self, domain: DomainId) -> Result<String> {
        let mut statement = self.connection.prepare("SELECT name FROM domain WHERE id = ?1")?;
        let name = statement.query_map([domain], |row| row.get(0))?.next().transpose()?;
        name.with_context(|| format!("No domain {domain}"))
    }

    /// Whether `domain` is under `ancestor`, directly or through other domains.
    fn is_under(&self, domain: DomainId, ancestor: DomainId) -> Result<bool> {
        let found = self.connection.query_row(
            "WITH RECURSIVE under (id) AS (
                 SELECT child FROM link WHERE parent = ?2
                 UNION
                 SELECT link.child FROM link JOIN under ON link.parent = under.id)
             SELECT EXISTS (SELECT 1 FROM under WHERE id = ?1)",
            (domain, ancestor),
            |row| row.get(0),
        )?;
        Ok(found)
    }
}

fn valid_name(name: &str) -> Result<&str> {
    let name = name.trim();
    ensure!(!name.is_empty(), "A domain needs a name");
    Ok(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    const MISSING: DomainId = DomainId::new(99);

    #[test]
    fn a_domain_can_have_several_parents() -> Result<()> {
        let mut store = Store::in_memory()?;
        let system = store.add_domain("System", None, None)?;
        let memory = store.add_domain(" Memory ", Some(system), None)?;
        let distributed = store.add_domain("Distributed system", Some(system), None)?;
        let cxl = store.add_domain("CXL", Some(memory), None)?;
        store.link(distributed, cxl)?;
        store.link(distributed, cxl)?;

        let links = [(system, memory), (system, distributed), (memory, cxl), (distributed, cxl)];
        assert_eq!(store.links()?, links);
        assert_eq!(store.domains()?[1].name, "Memory");
        assert!(store.add_domain("  ", None, None).is_err());
        assert!(store.add_domain("Orphan", Some(MISSING), None).is_err());
        assert!(store.add_domain("Lost", Some(MISSING), Some((0.0, 0.0))).is_err());
        Ok(())
    }

    #[test]
    fn a_link_that_would_form_a_loop_is_refused() -> Result<()> {
        let mut store = Store::in_memory()?;
        let top = store.add_domain("Top", None, None)?;
        let middle = store.add_domain("Middle", Some(top), None)?;
        let bottom = store.add_domain("Bottom", Some(middle), None)?;

        assert!(store.link(top, top).is_err());
        assert!(store.link(middle, top).is_err());
        assert!(store.link(bottom, top).is_err());
        assert!(store.link(top, MISSING).is_err());
        store.link(top, bottom)?;
        Ok(())
    }

    #[test]
    fn removing_the_last_link_makes_a_top_domain() -> Result<()> {
        let mut store = Store::in_memory()?;
        let top = store.add_domain("Top", None, None)?;
        let child = store.add_domain("Smaller", Some(top), None)?;

        store.unlink(top, child)?;

        assert!(store.links()?.is_empty());
        assert_eq!(store.domains()?.len(), 2);
        assert!(store.unlink(top, child).is_err());
        Ok(())
    }

    #[test]
    fn deleting_a_domain_moves_stranded_smaller_domains_up() -> Result<()> {
        let mut store = Store::in_memory()?;
        let top = store.add_domain("Top", None, None)?;
        let other = store.add_domain("Other", None, None)?;
        let middle = store.add_domain("Middle", Some(top), None)?;
        let stranded = store.add_domain("Stranded", Some(middle), None)?;
        let shared = store.add_domain("Shared", Some(middle), None)?;
        store.link(other, shared)?;

        store.delete_domain(middle)?;

        assert_eq!(store.links()?, [(top, stranded), (other, shared)]);
        assert!(store.delete_domain(middle).is_err());

        store.delete_domain(top)?;
        assert_eq!(store.links()?, [(other, shared)]);
        assert_eq!(store.domains()?.len(), 3);
        Ok(())
    }

    #[test]
    fn a_new_domain_appears_beside_its_larger_domain() -> Result<()> {
        let mut store = Store::in_memory()?;
        let first = store.add_domain("First", None, None)?;
        let second = store.add_domain("Second", None, None)?;
        let fixed = store.add_domain("Fixed", Some(first), Some((5.0, 6.0)))?;
        let near = store.add_domain("Near", Some(second), None)?;
        let next = store.add_domain("Next", Some(second), None)?;

        let domains = store.domains()?;
        let at =
            |id| domains.iter().find(|domain| domain.id == id).map(|domain| (domain.x, domain.y));
        assert_eq!(at(first), Some((0.0, 0.0)));
        assert_eq!(at(second), Some((420.0, 0.0)));
        assert_eq!(at(fixed), Some((5.0, 6.0)));
        let (near, next) = (at(near).context("near")?, at(next).context("next")?);
        assert!(((near.0 - 420.0).hypot(near.1) - 170.0).abs() < 1e-9);
        assert!((near.0 - next.0).hypot(near.1 - next.1) > 100.0);
        Ok(())
    }

    #[test]
    fn a_tree_of_seeds_arrives_whole_or_not_at_all() -> Result<()> {
        let mut store = Store::in_memory()?;
        let top = store.add_domain("Top", None, None)?;
        let seed = |name: &str, parent, x| Seed { name: name.into(), parent, x, y: 2.0 };
        let seeds = [seed("Root", None, 1.0), seed("Leaf", Some(0), 3.0)];

        let domains = store.add_domains(&seeds, Some(top))?;

        assert_eq!(store.links()?, [(top, domains[0]), (domains[0], domains[1])]);
        assert_eq!((store.domains()?[2].x, store.domains()?[2].y), (3.0, 2.0));
        assert!(store.add_domains(&seeds, Some(MISSING)).is_err());
        let unnamed = [seed("Fine", None, 0.0), seed(" ", Some(0), 0.0)];
        assert!(store.add_domains(&unnamed, None).is_err());
        assert_eq!(store.domains()?.len(), 3);
        Ok(())
    }

    #[test]
    fn domains_keep_names_and_positions_across_reopening() -> Result<()> {
        let folder = tempfile::tempdir()?;
        let path = folder.path().join("nested/deck.db");
        let mut store = Store::open(&path)?;
        let domain = store.add_domain("System", None, None)?;
        store.rename_domain(domain, "Systems")?;
        store.move_domain(domain, 12.5, -3.0)?;
        assert!(store.rename_domain(domain, "").is_err());
        assert!(store.move_domain(MISSING, 0.0, 0.0).is_err());
        drop(store);

        let store = Store::open(&path)?;

        let kept = Domain { id: domain, name: "Systems".into(), x: 12.5, y: -3.0 };
        assert_eq!(store.domains()?, [kept]);
        Ok(())
    }
}
