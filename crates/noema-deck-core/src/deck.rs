//! One running Deck: the universe and the shell panes, shared by every window
//! and connection.

use std::{path::Path, time::Duration};

use anyhow::{Context, Result};
use parking_lot::Mutex;

use crate::{
    agent::claude,
    pane::{self, Panes, login_environment},
    universe::{self, Snapshot, Universe},
};

/// How long a read waits for a pane's output before answering with none.
const READ_WAIT: Duration = Duration::from_secs(20);

pub struct Deck {
    universe: Mutex<Universe>,
    panes: Panes,
}

impl Deck {
    /// Opens the store at `store`, or the per-user one, beside Claude Code's
    /// own sessions.
    pub fn open(store: Option<&Path>) -> Result<Self> {
        Ok(Self { universe: Mutex::new(Universe::at(store)?), panes: Panes::default() })
    }

    /// Answers a request about the map.
    pub fn map(&self, request: universe::Request) -> Result<Snapshot> {
        self.universe.lock().handle(request)
    }

    /// Answers a request about a pane. The universe is held only for a moment
    /// around opening and closing, so a waiting read never holds up the map.
    pub fn pane(&self, request: pane::Request) -> Result<pane::Reply> {
        let mut reply = pane::Reply::default();
        match request {
            pane::Request::Open { session, rows, cols } => {
                let launch = {
                    let universe = self.universe.lock();
                    let session =
                        universe.session(&session).context("That session is no longer listed")?;
                    claude::open(session)?
                };
                reply.pane = Some(self.panes.open(&launch, login_environment(), rows, cols)?);
            }
            pane::Request::Start { star, folder, rows, cols } => {
                let (key, launch) = self.universe.lock().plan_start(star, &folder)?;
                let pane = self.panes.open(&launch, login_environment(), rows, cols)?;
                let recorded =
                    self.universe.lock().record_start(star, key.clone(), &launch.folder, pane);
                if recorded.is_err() {
                    self.panes.close(pane);
                }
                recorded?;
                (reply.pane, reply.session) = (Some(pane), Some(key));
            }
            pane::Request::Read { pane } => {
                let (data, exited) = self.panes.read(pane, READ_WAIT)?;
                (reply.data, reply.exited) = (Some(data), Some(exited));
            }
            pane::Request::Write { pane, data } => self.panes.write(pane, data.as_bytes())?,
            pane::Request::Resize { pane, rows, cols } => self.panes.resize(pane, rows, cols)?,
            pane::Request::Close { pane } => {
                self.panes.close(pane);
                self.universe.lock().settle(pane)?;
            }
        }
        Ok(reply)
    }
}
