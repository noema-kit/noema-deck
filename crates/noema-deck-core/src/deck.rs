//! One running Deck: the map and the shell panes, shared by every window
//! and connection.

use std::{path::Path, sync::Arc, thread, time::Duration};

use anyhow::{Context, Result};
use parking_lot::Mutex;

use crate::{
    agent::{self, Agent, Homes},
    folder,
    map::{self, Map, Snapshot},
    pane::{self, Panes, login_environment},
    store,
    usage::{Asker, Ledger, LimitsRequest, Plan, Report},
};

/// How long a read waits for a pane's output before answering with none.
const READ_WAIT: Duration = Duration::from_secs(20);

pub struct Deck {
    map: Mutex<Map>,
    panes: Panes,
    usage: Arc<Mutex<Ledger>>,
    asker: Arc<Asker>,
}

impl Deck {
    /// Opens the store at `store`, or the per-user one, beside the agents' own
    /// sessions.
    pub fn open(store: Option<&Path>) -> Result<Self> {
        // Asking the login shell for its environment can take a second, so it
        // is asked now and not when the first pane opens.
        thread::spawn(login_environment);
        // So is reading every installed font, for Settings.
        thread::spawn(crate::fonts::installed);
        let store = store.map_or_else(store::default_path, |store| Ok(store.to_owned()))?;
        let mut map = Map::open(&store, Homes::find()?)?;
        map.forget_stand_ins()?;
        let deck = Self {
            map: Mutex::new(map),
            panes: Panes::default(),
            usage: Arc::default(),
            asker: Arc::new(Asker::keeping(store.with_file_name("limits.json"))),
        };
        // So Settings opens on figures at hand: the agents' records are read,
        // and the agents asked for their limits, while nobody is waiting.
        let (homes, usage, asker) =
            (deck.map.lock().homes().clone(), Arc::clone(&deck.usage), Arc::clone(&deck.asker));
        thread::spawn(move || {
            let _ = usage.lock().report(&homes);
            asker.plans(&homes, true);
        });
        Ok(deck)
    }

    /// Answers a request about the map.
    pub fn map(&self, request: map::Request) -> Result<Snapshot> {
        self.map.lock().handle(request)
    }

    /// The tokens the agents used. The first answer reads every record, so the
    /// map is held only long enough to learn where the records are.
    pub fn usage(&self) -> Result<Report> {
        let homes = self.map.lock().homes().clone();
        self.usage.lock().report(&homes)
    }

    /// Each agent's plan limits, in the agents' order, or one agent's. With
    /// `ask`, Claude Code and Antigravity are asked first, unless they were
    /// lately, which takes each a few seconds.
    pub fn limits(&self, request: LimitsRequest) -> Result<Vec<Plan>> {
        let homes = self.map.lock().homes().clone();
        let codex = || self.usage.lock().codex_plan(&homes);
        Ok(match request.agent {
            Some(Agent::Claude) => vec![self.asker.claude(request.ask)],
            Some(Agent::Codex) => vec![codex()?],
            Some(Agent::Antigravity) => vec![self.asker.antigravity(&homes, request.ask)],
            None => {
                let [claude, antigravity] = self.asker.plans(&homes, request.ask);
                vec![claude, codex()?, antigravity]
            }
        })
    }

    /// Answers a request about a pane. The map is held only for a moment
    /// around opening and closing, so a waiting read never holds up the map.
    pub fn pane(&self, request: pane::Request) -> Result<pane::Reply> {
        let mut reply = pane::Reply::default();
        match request {
            pane::Request::Open { session, rows, cols, light } => {
                let launch = {
                    let map = self.map.lock();
                    let session =
                        map.session(&session).context("That session is no longer listed")?;
                    agent::for_pane(session.key.agent, agent::open(session)?, light)
                };
                reply.pane = Some(self.panes.open(&launch, login_environment(), rows, cols)?);
            }
            pane::Request::Start { domain, agent, folder, name, rows, cols, light } => {
                let mut plan =
                    self.map.lock().plan_start(domain, agent, &folder, name.as_deref())?;
                plan.launch = agent::for_pane(agent, plan.launch, light);
                let pane = self.panes.open(&plan.launch, login_environment(), rows, cols)?;
                let key = plan.key.clone();
                let recorded = self.map.lock().record_start(domain, plan, pane, name.as_deref());
                if recorded.is_err() {
                    self.panes.close(pane);
                }
                recorded?;
                (reply.pane, reply.session) = (Some(pane), Some(key));
            }
            pane::Request::Shell { folder, rows, cols } => {
                let launch = pane::shell(&folder::from_label(&folder))?;
                reply.pane = Some(self.panes.open(&launch, login_environment(), rows, cols)?);
            }
            pane::Request::Read { pane } => {
                let (data, exited) = self.panes.read(pane, READ_WAIT)?;
                (reply.data, reply.exited) = (Some(data), Some(exited));
            }
            pane::Request::Write { pane, data } => self.panes.write(pane, data.as_bytes())?,
            pane::Request::Resize { pane, rows, cols } => self.panes.resize(pane, rows, cols)?,
            pane::Request::Close { pane } => {
                self.panes.close(pane);
                self.map.lock().settle(pane)?;
            }
        }
        Ok(reply)
    }
}
