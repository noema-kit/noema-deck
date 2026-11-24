//! Which sessions belong to which domains.

use anyhow::{Result, ensure};
use rusqlite::Row;

use super::Store;
use crate::{domain::DomainId, session::SessionKey};

impl Store {
    pub fn assign(&self, domain: DomainId, session: &SessionKey) -> Result<()> {
        self.require(domain)?;
        self.connection.execute(
            "INSERT OR IGNORE INTO assignment (domain, agent, session) VALUES (?1, ?2, ?3)",
            (domain, session.agent.name(), &session.id),
        )?;
        Ok(())
    }

    pub fn unassign(&self, domain: DomainId, session: &SessionKey) -> Result<()> {
        let changed = self.connection.execute(
            "DELETE FROM assignment WHERE domain = ?1 AND agent = ?2 AND session = ?3",
            (domain, session.agent.name(), &session.id),
        )?;
        ensure!(changed == 1, "Session {} is not on domain {domain}", session.id);
        Ok(())
    }

    /// Removes a session from every domain.
    pub fn forget_session(&self, session: &SessionKey) -> Result<()> {
        self.connection.execute(
            "DELETE FROM assignment WHERE agent = ?1 AND session = ?2",
            (session.agent.name(), &session.id),
        )?;
        Ok(())
    }

    /// Every assignment as `(domain, session)`. A session absent from all of them
    /// is unassigned.
    pub fn assignments(&self) -> Result<Vec<(DomainId, SessionKey)>> {
        let mut statement = self.connection.prepare(
            "SELECT agent, session, domain FROM assignment ORDER BY domain, agent, session",
        )?;
        let mut rows = statement.query([])?;
        let mut assignments = Vec::new();
        while let Some(row) = rows.next()? {
            assignments.push((row.get(2)?, session_key(row)?));
        }
        Ok(assignments)
    }

    /// The distinct sessions on `domain`, and on every subdomain when
    /// `nested` is set. With it set, their number is the domain's count.
    pub fn sessions_on(&self, domain: DomainId, nested: bool) -> Result<Vec<SessionKey>> {
        self.require(domain)?;
        let mut statement = self.connection.prepare(
            "WITH RECURSIVE under (id) AS (
                 SELECT ?1
                 UNION
                 SELECT link.child FROM link JOIN under ON link.parent = under.id WHERE ?2)
             SELECT DISTINCT agent, session FROM assignment
             WHERE domain IN (SELECT id FROM under) ORDER BY agent, session",
        )?;
        let mut rows = statement.query((domain, nested))?;
        let mut sessions = Vec::new();
        while let Some(row) = rows.next()? {
            sessions.push(session_key(row)?);
        }
        Ok(sessions)
    }
}

/// Reads a session key from a row whose first two columns are its agent and ID.
fn session_key(row: &Row<'_>) -> Result<SessionKey> {
    let agent: String = row.get(0)?;
    Ok(SessionKey { agent: agent.parse()?, id: row.get(1)? })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::Agent;

    fn session(id: &str) -> SessionKey {
        SessionKey { agent: Agent::Claude, id: id.into() }
    }

    #[test]
    fn a_domain_counts_distinct_sessions_on_it_and_under_it() -> Result<()> {
        let mut store = Store::in_memory()?;
        let top = store.add_domain("Top", None, None)?;
        let left = store.add_domain("Left", Some(top), None)?;
        let right = store.add_domain("Right", Some(top), None)?;
        let shared = store.add_domain("Shared", Some(left), None)?;
        store.link(right, shared)?;
        store.assign(top, &session("a"))?;
        store.assign(left, &session("b"))?;
        store.assign(shared, &session("b"))?;
        store.assign(shared, &session("c"))?;
        store.assign(shared, &session("c"))?;

        assert_eq!(store.sessions_on(top, false)?, [session("a")]);
        assert_eq!(store.sessions_on(top, true)?, [session("a"), session("b"), session("c")]);
        assert_eq!(store.sessions_on(right, true)?, [session("b"), session("c")]);
        assert!(store.sessions_on(DomainId::new(99), true).is_err());
        assert!(store.assign(DomainId::new(99), &session("a")).is_err());

        store.unassign(shared, &session("b"))?;
        assert_eq!(store.sessions_on(right, true)?, [session("c")]);
        assert!(store.unassign(shared, &session("b")).is_err());
        assert_eq!(store.assignments()?.len(), 3);
        Ok(())
    }

    #[test]
    fn forgetting_a_session_removes_it_from_every_domain() -> Result<()> {
        let mut store = Store::in_memory()?;
        let first = store.add_domain("First", None, None)?;
        let second = store.add_domain("Second", None, None)?;
        store.assign(first, &session("gone"))?;
        store.assign(second, &session("gone"))?;
        store.assign(second, &session("kept"))?;

        store.forget_session(&session("gone"))?;

        assert_eq!(store.assignments()?, [(second, session("kept"))]);
        Ok(())
    }
}
