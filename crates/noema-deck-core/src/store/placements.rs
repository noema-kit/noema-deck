//! Which sessions sit on which stars.

use anyhow::{Result, ensure};
use rusqlite::Row;

use super::Store;
use crate::{session::SessionKey, star::StarId};

impl Store {
    pub fn place(&self, star: StarId, session: &SessionKey) -> Result<()> {
        self.require(star)?;
        self.connection.execute(
            "INSERT OR IGNORE INTO placement (star, agent, session) VALUES (?1, ?2, ?3)",
            (star, session.agent.name(), &session.id),
        )?;
        Ok(())
    }

    pub fn unplace(&self, star: StarId, session: &SessionKey) -> Result<()> {
        let changed = self.connection.execute(
            "DELETE FROM placement WHERE star = ?1 AND agent = ?2 AND session = ?3",
            (star, session.agent.name(), &session.id),
        )?;
        ensure!(changed == 1, "Session {} is not on star {star}", session.id);
        Ok(())
    }

    /// Takes a session off every star.
    pub fn forget_session(&self, session: &SessionKey) -> Result<()> {
        self.connection.execute(
            "DELETE FROM placement WHERE agent = ?1 AND session = ?2",
            (session.agent.name(), &session.id),
        )?;
        Ok(())
    }

    /// Every placement as `(star, session)`. A session absent from all of them
    /// is unsorted.
    pub fn placements(&self) -> Result<Vec<(StarId, SessionKey)>> {
        let mut statement = self
            .connection
            .prepare("SELECT agent, session, star FROM placement ORDER BY star, agent, session")?;
        let mut rows = statement.query([])?;
        let mut placements = Vec::new();
        while let Some(row) = rows.next()? {
            placements.push((row.get(2)?, session_key(row)?));
        }
        Ok(placements)
    }

    /// The distinct sessions on `star`, and on every star beneath it when
    /// `with_smaller` is set. With it set, their number is the star's count.
    pub fn sessions_on(&self, star: StarId, with_smaller: bool) -> Result<Vec<SessionKey>> {
        self.require(star)?;
        let mut statement = self.connection.prepare(
            "WITH RECURSIVE under (id) AS (
                 SELECT ?1
                 UNION
                 SELECT link.smaller FROM link JOIN under ON link.larger = under.id WHERE ?2)
             SELECT DISTINCT agent, session FROM placement
             WHERE star IN (SELECT id FROM under) ORDER BY agent, session",
        )?;
        let mut rows = statement.query((star, with_smaller))?;
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
    fn a_star_counts_distinct_sessions_on_it_and_beneath_it() -> Result<()> {
        let mut store = Store::in_memory()?;
        let top = store.add_star("Top", None, None)?;
        let left = store.add_star("Left", Some(top), None)?;
        let right = store.add_star("Right", Some(top), None)?;
        let shared = store.add_star("Shared", Some(left), None)?;
        store.link(right, shared)?;
        store.place(top, &session("a"))?;
        store.place(left, &session("b"))?;
        store.place(shared, &session("b"))?;
        store.place(shared, &session("c"))?;
        store.place(shared, &session("c"))?;

        assert_eq!(store.sessions_on(top, false)?, [session("a")]);
        assert_eq!(store.sessions_on(top, true)?, [session("a"), session("b"), session("c")]);
        assert_eq!(store.sessions_on(right, true)?, [session("b"), session("c")]);
        assert!(store.sessions_on(StarId::new(99), true).is_err());
        assert!(store.place(StarId::new(99), &session("a")).is_err());

        store.unplace(shared, &session("b"))?;
        assert_eq!(store.sessions_on(right, true)?, [session("c")]);
        assert!(store.unplace(shared, &session("b")).is_err());
        assert_eq!(store.placements()?.len(), 3);
        Ok(())
    }

    #[test]
    fn forgetting_a_session_takes_it_off_every_star() -> Result<()> {
        let mut store = Store::in_memory()?;
        let first = store.add_star("First", None, None)?;
        let second = store.add_star("Second", None, None)?;
        store.place(first, &session("gone"))?;
        store.place(second, &session("gone"))?;
        store.place(second, &session("kept"))?;

        store.forget_session(&session("gone"))?;

        assert_eq!(store.placements()?, [(second, session("kept"))]);
        Ok(())
    }
}
