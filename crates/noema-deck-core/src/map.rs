//! The map: every domain, link and assignment, joined with the sessions the
//! agent lists. One request goes in and one snapshot comes out.

mod arrange;
mod request;
mod snapshot;

use std::{
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use anyhow::{Context, Result};
use uuid::Uuid;

pub use self::{
    request::Request,
    snapshot::{DomainView, MissingSession, SessionView, Snapshot},
};
use crate::{
    agent::{Agent, Launch, claude},
    domain::DomainId,
    folder, import,
    pane::PaneId,
    session::{Session, SessionKey},
    store::{self, Store},
};

pub struct Map {
    store: Store,
    projects: PathBuf,
    listing: claude::Listing,
    /// The sessions Deck started that the agent has not stored yet, then every
    /// session the agent lists.
    sessions: Vec<Session>,
    /// Sessions Deck started that the agent has not stored yet. Claude Code
    /// stores a session at its first prompt.
    started: Vec<Started>,
}

struct Started {
    /// The pane the session runs in.
    pane: PaneId,
    key: SessionKey,
    /// The name it was given, written to the session once the agent stores it.
    name: Option<String>,
}

impl Map {
    /// Opens the store at `store` and reads Claude Code's sessions from
    /// `projects`.
    pub fn open(store: &Path, projects: PathBuf) -> Result<Self> {
        let mut map = Self {
            store: Store::open(store)?,
            projects,
            listing: claude::Listing::default(),
            sessions: Vec::new(),
            started: Vec::new(),
        };
        map.refresh()?;
        Ok(map)
    }

    /// Opens the store at `store`, or the per-user one, beside Claude Code's
    /// own sessions.
    pub fn at(store: Option<&Path>) -> Result<Self> {
        let store = match store {
            Some(store) => store.to_owned(),
            None => store::default_path()?,
        };
        Self::open(&store, claude::projects_dir()?)
    }

    /// Carries out a request and answers with the map as it then stands.
    pub fn handle(&mut self, request: Request) -> Result<Snapshot> {
        match request {
            Request::Snapshot => {}
            Request::Refresh => self.refresh()?,
            Request::AddDomain { name, parent, position } => {
                self.store.add_domain(&name, parent, position)?;
            }
            Request::Import { folder, parent, position, depth } => {
                let depth = depth.unwrap_or(import::DEPTH);
                self.import(&folder::from_label(&folder), parent, position, depth)?;
            }
            Request::RenameDomain { domain, name } => self.store.rename_domain(domain, &name)?,
            Request::MoveDomain { domain, x, y } => self.store.move_domain(domain, x, y)?,
            Request::Arrange => {
                let places = arrange::arrangement(&self.store.domains()?, &self.store.links()?);
                self.store.move_domains(&places)?;
            }
            Request::MoveDomains { moves } => self.store.move_domains(&moves)?,
            Request::DeleteDomain { domain } => self.store.delete_domain(domain)?,
            Request::Link { parent, child } => self.store.link(parent, child)?,
            Request::Unlink { parent, child } => self.store.unlink(parent, child)?,
            Request::Assign { domain, sessions } => {
                for session in &sessions {
                    self.store.assign(domain, session)?;
                }
            }
            Request::Unassign { domain, session } => self.store.unassign(domain, &session)?,
            Request::RenameSession { session, title } => self.rename_session(&session, &title)?,
            Request::KeepPanes { panes } => self.store.set_open_panes(panes)?,
            Request::SetAppearance { appearance } => self.store.set_appearance(appearance)?,
        }
        self.snapshot()
    }

    pub fn snapshot(&self) -> Result<Snapshot> {
        Snapshot::of(&self.store, &self.sessions)
    }

    /// A session known at the last refresh.
    #[must_use]
    pub fn session(&self, key: &SessionKey) -> Option<&Session> {
        self.sessions.iter().find(|session| &session.key == key)
    }

    /// Adds a domain and answers with its ID.
    pub fn add_domain(&mut self, name: &str, parent: Option<DomainId>) -> Result<DomainId> {
        self.store.add_domain(name, parent, None)
    }

    /// Adds `folder` and the folders up to `depth` levels inside it as a tree
    /// of domains, and answers with how many were added.
    pub fn import(
        &mut self,
        folder: &Path,
        parent: Option<DomainId>,
        position: Option<(f64, f64)>,
        depth: usize,
    ) -> Result<usize> {
        let origin = match position {
            Some(position) => position,
            None => self.store.free_spot(parent)?,
        };
        let seeds = import::seeds(folder, depth, origin)?;
        Ok(self.store.add_domains(&seeds, parent)?.len())
    }

    /// Chooses the ID of a new session on `domain` and the command that starts
    /// it in `folder`, under `name` if one is given. Nothing is recorded until
    /// the command is running.
    pub fn plan_start(
        &self,
        domain: DomainId,
        folder: &str,
        name: Option<&str>,
    ) -> Result<(SessionKey, Launch)> {
        self.store.require(domain)?;
        let key = SessionKey { agent: Agent::Claude, id: Uuid::new_v4().to_string() };
        let launch = claude::start(&key.id, &folder::from_label(folder), name)?;
        Ok((key, launch))
    }

    /// Assigns the session now running in `pane` to `domain`, so it shows there
    /// before the agent has stored it.
    pub fn record_start(
        &mut self,
        domain: DomainId,
        key: SessionKey,
        folder: &Path,
        pane: PaneId,
        name: Option<&str>,
    ) -> Result<()> {
        self.store.assign(domain, &key)?;
        self.store.set_last_folder(folder)?;
        let now = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |now| now.as_secs());
        let name = name.and_then(claude::clean_title);
        let session = Session {
            key: key.clone(),
            title: name.clone().unwrap_or_else(|| "New session".into()),
            folder: Some(folder.into()),
            updated: i64::try_from(now).unwrap_or(0),
        };
        self.sessions.insert(0, session);
        self.started.push(Started { pane, key, name });
        Ok(())
    }

    /// Tidies up after `pane` closed. A session started there that never got a
    /// prompt was never stored, so it leaves its domains instead of going missing.
    pub fn settle(&mut self, pane: PaneId) -> Result<()> {
        self.refresh()?;
        let Some(index) = self.started.iter().position(|started| started.pane == pane) else {
            return Ok(());
        };
        let Started { key, .. } = self.started.remove(index);
        self.sessions.retain(|session| session.key != key);
        self.store.forget_session(&key)
    }

    /// Names a session in its transcript, where Claude Code reads it too. A
    /// session not stored yet keeps the name until it is.
    fn rename_session(&mut self, key: &SessionKey, title: &str) -> Result<()> {
        if let Some(started) = self.started.iter_mut().find(|started| &started.key == key) {
            let name = claude::clean_title(title).context("Enter a name for the session")?;
            for session in self.sessions.iter_mut().filter(|session| &session.key == key) {
                session.title.clone_from(&name);
            }
            started.name = Some(name);
            return Ok(());
        }
        let transcript = self
            .listing
            .transcript(&key.id)
            .context("That session is no longer listed")?
            .to_owned();
        claude::rename(&transcript, &key.id, title)?;
        self.refresh()
    }

    fn refresh(&mut self) -> Result<()> {
        let mut listed = self.listing.sessions(&self.projects)?;
        // A named session the agent has just stored takes its name now.
        for started in &self.started {
            let Some(name) = &started.name else { continue };
            let Some(session) = listed.iter_mut().find(|session| session.key == started.key) else {
                continue;
            };
            if let Some(transcript) = self.listing.transcript(&session.key.id)
                && &session.title != name
            {
                session.title = claude::rename(transcript, &session.key.id, name)?;
            }
        }
        self.started.retain(|started| !listed.iter().any(|session| session.key == started.key));
        let waiting =
            |session: &Session| self.started.iter().any(|started| started.key == session.key);
        self.sessions.retain(waiting);
        self.sessions.extend(listed);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use anyhow::Context;
    use serde_json::{Value, json};

    use super::*;

    const LISTED: &str = "11111111-1111-4111-8111-111111111111";
    const OTHER: &str = "22222222-2222-4222-8222-222222222222";
    const PANE: PaneId = PaneId::new(7);

    fn map(folder: &Path) -> Result<Map> {
        let project = folder.join("projects/p");
        fs::create_dir_all(&project)?;
        for (id, title) in [(LISTED, "Listed"), (OTHER, "Other")] {
            let line = json!({ "type": "ai-title", "aiTitle": title, "cwd": "/work" });
            fs::write(project.join(format!("{id}.jsonl")), line.to_string())?;
        }
        Map::open(&folder.join("deck.db"), folder.join("projects"))
    }

    /// Sends a request as the interface would and reads the answer the same way.
    fn ask(map: &mut Map, command: &str, args: Value) -> Result<Value> {
        let fields = [("command".to_owned(), command.into()), ("args".to_owned(), args)];
        let request = serde_json::from_value(Value::Object(fields.into_iter().collect()))?;
        Ok(serde_json::to_value(map.handle(request)?)?)
    }

    fn snapshot(map: &Map) -> Result<Value> {
        Ok(serde_json::to_value(map.snapshot()?)?)
    }

    #[test]
    fn a_snapshot_rolls_sessions_up_to_larger_domains() -> Result<()> {
        let folder = tempfile::tempdir()?;
        let mut map = map(folder.path())?;
        let key = json!({ "agent": "claude", "id": LISTED });
        ask(&mut map, "add_domain", json!({ "name": "Top" }))?;
        ask(&mut map, "add_domain", json!({ "name": "Small", "parent": 1 }))?;

        let snapshot = ask(&mut map, "assign", json!({ "domain": 2, "sessions": [key] }))?;

        assert_eq!(snapshot["links"], json!([[1, 2]]));
        assert_eq!(snapshot["domains"][0]["sessions"], json!([key]));
        assert!(snapshot["domains"][0]["updated"].is_i64());
        let sessions = snapshot["sessions"].as_array().context("sessions")?;
        let listed = sessions.iter().find(|session| session["id"] == LISTED).context("listed")?;
        assert_eq!(listed["domains"], json!([2]));
        assert_eq!(listed["title"], "Listed");
        assert_eq!(listed["folder"], "/work");
        let unassigned = sessions.iter().filter(|session| session["domains"] == json!([])).count();
        assert_eq!(unassigned, 1);
        Ok(())
    }

    #[test]
    fn a_assigned_session_the_agent_no_longer_lists_is_missing() -> Result<()> {
        let folder = tempfile::tempdir()?;
        let mut map = map(folder.path())?;
        let key = json!({ "agent": "claude", "id": OTHER });
        ask(&mut map, "add_domain", json!({ "name": "Top", "position": [3.5, -2.0] }))?;
        ask(&mut map, "assign", json!({ "domain": 1, "sessions": [key] }))?;
        fs::remove_file(folder.path().join(format!("projects/p/{OTHER}.jsonl")))?;

        let request = serde_json::from_value(json!({ "command": "refresh" }))?;
        let snapshot = serde_json::to_value(map.handle(request)?)?;

        assert_eq!(
            snapshot["missing"],
            json!([{ "agent": "claude", "id": OTHER, "domains": [1] }])
        );
        assert_eq!(snapshot["domains"][0]["updated"], Value::Null);
        assert_eq!(snapshot["domains"][0]["x"], 3.5);
        assert_eq!(snapshot["domains"][0]["y"], -2.0);

        let unassigned = ask(&mut map, "unassign", json!({ "domain": 1, "session": key }))?;
        assert_eq!(unassigned["missing"], json!([]));
        Ok(())
    }

    #[test]
    fn deleting_a_domain_takes_its_assignments_with_it() -> Result<()> {
        let folder = tempfile::tempdir()?;
        let mut map = map(folder.path())?;
        let key = json!({ "agent": "claude", "id": LISTED });
        ask(&mut map, "add_domain", json!({ "name": "Top" }))?;
        ask(&mut map, "assign", json!({ "domain": 1, "sessions": [key] }))?;

        let snapshot = ask(&mut map, "delete_domain", json!({ "domain": 1 }))?;

        assert_eq!(snapshot["domains"], json!([]));
        let sessions = snapshot["sessions"].as_array().context("sessions")?;
        assert!(sessions.iter().all(|session| session["domains"] == json!([])));
        assert_eq!(snapshot["missing"], json!([]));
        Ok(())
    }

    #[test]
    fn importing_a_folder_adds_its_tree_under_a_domain() -> Result<()> {
        let folder = tempfile::tempdir()?;
        let mut map = map(folder.path())?;
        fs::create_dir_all(folder.path().join("topics/os/kernel"))?;
        ask(&mut map, "add_domain", json!({ "name": "Top" }))?;
        let topics = folder.path().join("topics");
        let args = json!({ "folder": topics, "parent": 1, "position": [10.0, 20.0] });

        let snapshot = ask(&mut map, "import", args)?;

        let domains = snapshot["domains"].as_array().context("domains")?;
        let names: Vec<_> = domains.iter().map(|domain| &domain["name"]).collect();
        assert_eq!(names, ["Top", "topics", "os", "kernel"]);
        assert_eq!(snapshot["links"], json!([[1, 2], [2, 3], [3, 4]]));
        assert_eq!((&domains[1]["x"], &domains[1]["y"]), (&json!(10.0), &json!(20.0)));

        let shallow = ask(&mut map, "import", json!({ "folder": topics, "depth": 0 }))?;
        assert_eq!(shallow["domains"].as_array().map(Vec::len), Some(5));
        assert!(ask(&mut map, "import", json!({ "folder": "/no/such" })).is_err());
        Ok(())
    }

    #[test]
    fn a_started_session_joins_its_domain_before_the_agent_stores_it() -> Result<()> {
        let folder = tempfile::tempdir()?;
        let mut map = map(folder.path())?;
        let top = map.add_domain("Top", None)?;
        let work = folder.path().to_str().context("folder name")?;

        let (key, launch) = map.plan_start(top, work, None)?;
        map.record_start(top, key.clone(), &launch.folder, PANE, None)?;
        let snapshot = snapshot(&map)?;

        assert_eq!(launch.args, ["--session-id", key.id.as_str()]);
        let started = &snapshot["sessions"][0];
        assert_eq!((&started["id"], &started["title"]), (&json!(key.id), &json!("New session")));
        assert_eq!(started["domains"], json!([1]));
        assert_eq!(
            snapshot["domains"][0]["sessions"],
            json!([{ "agent": "claude", "id": key.id }])
        );
        assert_eq!(snapshot["missing"], json!([]));
        assert_eq!(snapshot["last_folder"], json!(folder::label(folder.path())));
        assert!(map.plan_start(DomainId::new(9), work, None).is_err());
        assert!(map.plan_start(top, "/no/such/folder", None).is_err());
        Ok(())
    }

    #[test]
    fn a_started_session_stays_only_if_the_agent_stored_it() -> Result<()> {
        let folder = tempfile::tempdir()?;
        let mut map = map(folder.path())?;
        let top = map.add_domain("Top", None)?;
        let work = folder.path().to_str().context("folder name")?;
        let (used, launch) = map.plan_start(top, work, None)?;
        map.record_start(top, used.clone(), &launch.folder, PaneId::new(1), None)?;
        let (unused, launch) = map.plan_start(top, work, None)?;
        map.record_start(top, unused.clone(), &launch.folder, PaneId::new(2), None)?;
        let line = json!({ "type": "ai-title", "aiTitle": "Used", "cwd": work });
        let transcript = folder.path().join(format!("projects/p/{}.jsonl", used.id));
        fs::write(transcript, line.to_string())?;

        map.settle(PaneId::new(1))?;
        map.settle(PaneId::new(2))?;
        let snapshot = snapshot(&map)?;

        let sessions = snapshot["sessions"].as_array().context("sessions")?;
        let assigned: Vec<_> =
            sessions.iter().filter(|session| session["domains"] == json!([1])).collect();
        assert_eq!(assigned.len(), 1);
        assert_eq!((&assigned[0]["id"], &assigned[0]["title"]), (&json!(used.id), &json!("Used")));
        assert!(sessions.iter().all(|session| session["id"] != json!(unused.id)));
        assert_eq!(snapshot["missing"], json!([]));
        assert_eq!(snapshot["domains"][0]["sessions"].as_array().map(Vec::len), Some(1));
        Ok(())
    }

    #[test]
    fn a_session_is_renamed_in_its_transcript_once_stored() -> Result<()> {
        let folder = tempfile::tempdir()?;
        let mut map = map(folder.path())?;
        let key = json!({ "agent": "claude", "id": LISTED });

        let snapshot =
            ask(&mut map, "rename_session", json!({ "session": key, "title": "Named" }))?;

        let sessions = snapshot["sessions"].as_array().context("sessions")?;
        let named = sessions.iter().find(|session| session["id"] == LISTED).context("named")?;
        assert_eq!(named["title"], "Named");
        let top = map.add_domain("Top", None)?;
        let work = folder.path().to_str().context("folder name")?;
        let (unstored, launch) = map.plan_start(top, work, None)?;
        map.record_start(top, unstored.clone(), &launch.folder, PANE, None)?;
        let args = json!({ "session": unstored, "title": "Before its prompt" });
        let snapshot = ask(&mut map, "rename_session", args)?;
        assert_eq!(snapshot["sessions"][0]["title"], "Before its prompt");

        // Its first prompt stores it, and the name is written to it then.
        let line = json!({ "type": "user", "cwd": work, "message": { "content": "hello" } });
        let transcript = folder.path().join(format!("projects/p/{}.jsonl", unstored.id));
        fs::write(&transcript, format!("{line}\n"))?;
        let snapshot = ask(&mut map, "refresh", Value::Null)?;
        let stored = snapshot["sessions"]
            .as_array()
            .context("sessions")?
            .iter()
            .find(|session| session["id"] == json!(unstored.id))
            .context("stored")?;
        assert_eq!(stored["title"], "Before its prompt");
        assert!(fs::read_to_string(transcript)?.contains("custom-title"));

        Ok(())
    }

    #[test]
    fn open_panes_come_back_in_the_snapshot() -> Result<()> {
        let folder = tempfile::tempdir()?;
        let mut map = map(folder.path())?;
        let panes = json!({
            "open": [
                { "kind": "session", "session": { "agent": "claude", "id": LISTED } },
                { "kind": "shell", "folder": "~", "name": "Logs" },
            ],
            "layout": { "dir": "column", "items": [0, 1], "sizes": [0.7, 0.3] },
        });

        let snapshot = ask(&mut map, "keep_panes", json!({ "panes": panes }))?;

        assert_eq!(snapshot["panes"], panes);
        Ok(())
    }

    #[test]
    fn a_refused_request_reports_why_and_changes_nothing() -> Result<()> {
        let folder = tempfile::tempdir()?;
        let mut map = map(folder.path())?;
        ask(&mut map, "add_domain", json!({ "name": "Top" }))?;
        ask(&mut map, "add_domain", json!({ "name": "Small", "parent": 1 }))?;

        let refused = ask(&mut map, "link", json!({ "parent": 2, "child": 1 }));

        assert!(refused.is_err_and(|error| error.to_string().contains("loop")));
        assert_eq!(snapshot(&map)?["links"], json!([[1, 2]]));
        assert!(ask(&mut map, "explode", json!({})).is_err());
        Ok(())
    }
}
