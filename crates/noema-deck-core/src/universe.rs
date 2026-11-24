//! The universe: every star, link and placement, joined with the sessions the
//! agent lists. One request goes in and one snapshot comes out.

mod request;
mod snapshot;

use std::{
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use anyhow::Result;
use uuid::Uuid;

pub use self::{
    request::Request,
    snapshot::{MissingSession, SessionView, Snapshot, StarView},
};
use crate::{
    agent::{Agent, Launch, claude},
    folder, import,
    pane::PaneId,
    session::{Session, SessionKey},
    star::StarId,
    store::{self, Store},
};

pub struct Universe {
    store: Store,
    projects: PathBuf,
    listing: claude::Listing,
    /// The sessions Deck started that the agent has not stored yet, then every
    /// session the agent lists.
    sessions: Vec<Session>,
    /// Sessions Deck started that the agent has not stored yet, each with the
    /// pane it runs in. Claude Code stores a session at its first prompt.
    started: Vec<(PaneId, SessionKey)>,
}

impl Universe {
    /// Opens the store at `store` and reads Claude Code's sessions from
    /// `projects`.
    pub fn open(store: &Path, projects: PathBuf) -> Result<Self> {
        let mut universe = Self {
            store: Store::open(store)?,
            projects,
            listing: claude::Listing::default(),
            sessions: Vec::new(),
            started: Vec::new(),
        };
        universe.refresh()?;
        Ok(universe)
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

    /// Carries out a request and answers with the universe as it then stands.
    pub fn handle(&mut self, request: Request) -> Result<Snapshot> {
        match request {
            Request::Snapshot => {}
            Request::Refresh => self.refresh()?,
            Request::AddStar { name, larger, position } => {
                self.store.add_star(&name, larger, position)?;
            }
            Request::Import { folder, larger, position, depth } => {
                let depth = depth.unwrap_or(import::DEPTH);
                self.import(&folder::from_label(&folder), larger, position, depth)?;
            }
            Request::RenameStar { star, name } => self.store.rename_star(star, &name)?,
            Request::MoveStar { star, x, y } => self.store.move_star(star, x, y)?,
            Request::DeleteStar { star } => self.store.delete_star(star)?,
            Request::Link { larger, smaller } => self.store.link(larger, smaller)?,
            Request::Unlink { larger, smaller } => self.store.unlink(larger, smaller)?,
            Request::Place { star, sessions } => {
                for session in &sessions {
                    self.store.place(star, session)?;
                }
            }
            Request::Unplace { star, session } => self.store.unplace(star, &session)?,
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

    /// Adds a star and answers with its ID.
    pub fn add_star(&mut self, name: &str, larger: Option<StarId>) -> Result<StarId> {
        self.store.add_star(name, larger, None)
    }

    /// Adds `folder` and the folders up to `depth` levels inside it as a tree
    /// of stars, and answers with how many were added.
    pub fn import(
        &mut self,
        folder: &Path,
        larger: Option<StarId>,
        position: Option<(f64, f64)>,
        depth: usize,
    ) -> Result<usize> {
        let origin = match position {
            Some(position) => position,
            None => self.store.free_spot(larger)?,
        };
        let seeds = import::seeds(folder, depth, origin)?;
        Ok(self.store.add_stars(&seeds, larger)?.len())
    }

    /// Chooses the ID of a new session on `star` and the command that starts
    /// it in `folder`. Nothing is recorded until the command is running.
    pub fn plan_start(&self, star: StarId, folder: &str) -> Result<(SessionKey, Launch)> {
        self.store.require(star)?;
        let key = SessionKey { agent: Agent::Claude, id: Uuid::new_v4().to_string() };
        let launch = claude::start(&key.id, &folder::from_label(folder))?;
        Ok((key, launch))
    }

    /// Places the session now running in `pane` on `star`, so it shows there
    /// before the agent has stored it.
    pub fn record_start(
        &mut self,
        star: StarId,
        key: SessionKey,
        folder: &Path,
        pane: PaneId,
    ) -> Result<()> {
        self.store.place(star, &key)?;
        self.store.set_last_folder(folder)?;
        let now = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |now| now.as_secs());
        let session = Session {
            key: key.clone(),
            title: "New session".into(),
            folder: Some(folder.into()),
            updated: i64::try_from(now).unwrap_or(0),
        };
        self.sessions.insert(0, session);
        self.started.push((pane, key));
        Ok(())
    }

    /// Tidies up after `pane` closed. A session started there that never got a
    /// prompt was never stored, so it leaves its stars instead of going missing.
    pub fn settle(&mut self, pane: PaneId) -> Result<()> {
        self.refresh()?;
        let Some(index) = self.started.iter().position(|(started, _)| *started == pane) else {
            return Ok(());
        };
        let (_, key) = self.started.remove(index);
        self.sessions.retain(|session| session.key != key);
        self.store.forget_session(&key)
    }

    fn refresh(&mut self) -> Result<()> {
        let listed = self.listing.sessions(&self.projects)?;
        self.started.retain(|(_, key)| !listed.iter().any(|session| &session.key == key));
        let waiting = |session: &Session| self.started.iter().any(|(_, key)| key == &session.key);
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

    fn universe(folder: &Path) -> Result<Universe> {
        let project = folder.join("projects/p");
        fs::create_dir_all(&project)?;
        for (id, title) in [(LISTED, "Listed"), (OTHER, "Other")] {
            let line = json!({ "type": "ai-title", "aiTitle": title, "cwd": "/work" });
            fs::write(project.join(format!("{id}.jsonl")), line.to_string())?;
        }
        Universe::open(&folder.join("deck.db"), folder.join("projects"))
    }

    /// Sends a request as the interface would and reads the answer the same way.
    fn ask(universe: &mut Universe, command: &str, args: Value) -> Result<Value> {
        let fields = [("command".to_owned(), command.into()), ("args".to_owned(), args)];
        let request = serde_json::from_value(Value::Object(fields.into_iter().collect()))?;
        Ok(serde_json::to_value(universe.handle(request)?)?)
    }

    fn snapshot(universe: &Universe) -> Result<Value> {
        Ok(serde_json::to_value(universe.snapshot()?)?)
    }

    #[test]
    fn a_snapshot_rolls_sessions_up_to_larger_stars() -> Result<()> {
        let folder = tempfile::tempdir()?;
        let mut universe = universe(folder.path())?;
        let key = json!({ "agent": "claude", "id": LISTED });
        ask(&mut universe, "add_star", json!({ "name": "Top" }))?;
        ask(&mut universe, "add_star", json!({ "name": "Small", "larger": 1 }))?;

        let snapshot = ask(&mut universe, "place", json!({ "star": 2, "sessions": [key] }))?;

        assert_eq!(snapshot["links"], json!([[1, 2]]));
        assert_eq!(snapshot["stars"][0]["sessions"], json!([key]));
        assert!(snapshot["stars"][0]["updated"].is_i64());
        let sessions = snapshot["sessions"].as_array().context("sessions")?;
        let listed = sessions.iter().find(|session| session["id"] == LISTED).context("listed")?;
        assert_eq!(listed["stars"], json!([2]));
        assert_eq!(listed["title"], "Listed");
        assert_eq!(listed["folder"], "/work");
        let unsorted = sessions.iter().filter(|session| session["stars"] == json!([])).count();
        assert_eq!(unsorted, 1);
        Ok(())
    }

    #[test]
    fn a_placed_session_the_agent_no_longer_lists_is_missing() -> Result<()> {
        let folder = tempfile::tempdir()?;
        let mut universe = universe(folder.path())?;
        let key = json!({ "agent": "claude", "id": OTHER });
        ask(&mut universe, "add_star", json!({ "name": "Top", "position": [3.5, -2.0] }))?;
        ask(&mut universe, "place", json!({ "star": 1, "sessions": [key] }))?;
        fs::remove_file(folder.path().join(format!("projects/p/{OTHER}.jsonl")))?;

        let request = serde_json::from_value(json!({ "command": "refresh" }))?;
        let snapshot = serde_json::to_value(universe.handle(request)?)?;

        assert_eq!(snapshot["missing"], json!([{ "agent": "claude", "id": OTHER, "stars": [1] }]));
        assert_eq!(snapshot["stars"][0]["updated"], Value::Null);
        assert_eq!(snapshot["stars"][0]["x"], 3.5);
        assert_eq!(snapshot["stars"][0]["y"], -2.0);

        let unplaced = ask(&mut universe, "unplace", json!({ "star": 1, "session": key }))?;
        assert_eq!(unplaced["missing"], json!([]));
        Ok(())
    }

    #[test]
    fn deleting_a_star_takes_its_placements_with_it() -> Result<()> {
        let folder = tempfile::tempdir()?;
        let mut universe = universe(folder.path())?;
        let key = json!({ "agent": "claude", "id": LISTED });
        ask(&mut universe, "add_star", json!({ "name": "Top" }))?;
        ask(&mut universe, "place", json!({ "star": 1, "sessions": [key] }))?;

        let snapshot = ask(&mut universe, "delete_star", json!({ "star": 1 }))?;

        assert_eq!(snapshot["stars"], json!([]));
        let sessions = snapshot["sessions"].as_array().context("sessions")?;
        assert!(sessions.iter().all(|session| session["stars"] == json!([])));
        assert_eq!(snapshot["missing"], json!([]));
        Ok(())
    }

    #[test]
    fn importing_a_folder_adds_its_tree_under_a_star() -> Result<()> {
        let folder = tempfile::tempdir()?;
        let mut universe = universe(folder.path())?;
        fs::create_dir_all(folder.path().join("topics/os/kernel"))?;
        ask(&mut universe, "add_star", json!({ "name": "Top" }))?;
        let topics = folder.path().join("topics");
        let args = json!({ "folder": topics, "larger": 1, "position": [10.0, 20.0] });

        let snapshot = ask(&mut universe, "import", args)?;

        let stars = snapshot["stars"].as_array().context("stars")?;
        let names: Vec<_> = stars.iter().map(|star| &star["name"]).collect();
        assert_eq!(names, ["Top", "topics", "os", "kernel"]);
        assert_eq!(snapshot["links"], json!([[1, 2], [2, 3], [3, 4]]));
        assert_eq!((&stars[1]["x"], &stars[1]["y"]), (&json!(10.0), &json!(20.0)));

        let shallow = ask(&mut universe, "import", json!({ "folder": topics, "depth": 0 }))?;
        assert_eq!(shallow["stars"].as_array().map(Vec::len), Some(5));
        assert!(ask(&mut universe, "import", json!({ "folder": "/no/such" })).is_err());
        Ok(())
    }

    #[test]
    fn a_started_session_sits_on_its_star_before_the_agent_stores_it() -> Result<()> {
        let folder = tempfile::tempdir()?;
        let mut universe = universe(folder.path())?;
        let top = universe.add_star("Top", None)?;
        let work = folder.path().to_str().context("folder name")?;

        let (key, launch) = universe.plan_start(top, work)?;
        universe.record_start(top, key.clone(), &launch.folder, PANE)?;
        let snapshot = snapshot(&universe)?;

        assert_eq!(launch.args, ["--session-id", key.id.as_str()]);
        let started = &snapshot["sessions"][0];
        assert_eq!((&started["id"], &started["title"]), (&json!(key.id), &json!("New session")));
        assert_eq!(started["stars"], json!([1]));
        assert_eq!(snapshot["stars"][0]["sessions"], json!([{ "agent": "claude", "id": key.id }]));
        assert_eq!(snapshot["missing"], json!([]));
        assert_eq!(snapshot["last_folder"], json!(folder::label(folder.path())));
        assert!(universe.plan_start(StarId::new(9), work).is_err());
        assert!(universe.plan_start(top, "/no/such/folder").is_err());
        Ok(())
    }

    #[test]
    fn a_started_session_stays_only_if_the_agent_stored_it() -> Result<()> {
        let folder = tempfile::tempdir()?;
        let mut universe = universe(folder.path())?;
        let top = universe.add_star("Top", None)?;
        let work = folder.path().to_str().context("folder name")?;
        let (used, launch) = universe.plan_start(top, work)?;
        universe.record_start(top, used.clone(), &launch.folder, PaneId::new(1))?;
        let (unused, launch) = universe.plan_start(top, work)?;
        universe.record_start(top, unused.clone(), &launch.folder, PaneId::new(2))?;
        let line = json!({ "type": "ai-title", "aiTitle": "Used", "cwd": work });
        let transcript = folder.path().join(format!("projects/p/{}.jsonl", used.id));
        fs::write(transcript, line.to_string())?;

        universe.settle(PaneId::new(1))?;
        universe.settle(PaneId::new(2))?;
        let snapshot = snapshot(&universe)?;

        let sessions = snapshot["sessions"].as_array().context("sessions")?;
        let placed: Vec<_> =
            sessions.iter().filter(|session| session["stars"] == json!([1])).collect();
        assert_eq!(placed.len(), 1);
        assert_eq!((&placed[0]["id"], &placed[0]["title"]), (&json!(used.id), &json!("Used")));
        assert!(sessions.iter().all(|session| session["id"] != json!(unused.id)));
        assert_eq!(snapshot["missing"], json!([]));
        assert_eq!(snapshot["stars"][0]["sessions"].as_array().map(Vec::len), Some(1));
        Ok(())
    }

    #[test]
    fn a_refused_request_reports_why_and_changes_nothing() -> Result<()> {
        let folder = tempfile::tempdir()?;
        let mut universe = universe(folder.path())?;
        ask(&mut universe, "add_star", json!({ "name": "Top" }))?;
        ask(&mut universe, "add_star", json!({ "name": "Small", "larger": 1 }))?;

        let refused = ask(&mut universe, "link", json!({ "larger": 2, "smaller": 1 }));

        assert!(refused.is_err_and(|error| error.to_string().contains("loop")));
        assert_eq!(snapshot(&universe)?["links"], json!([[1, 2]]));
        assert!(ask(&mut universe, "explode", json!({})).is_err());
        Ok(())
    }
}
