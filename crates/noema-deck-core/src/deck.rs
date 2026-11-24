//! One running Deck: the map and the shell panes, shared by every window
//! and connection.

use std::{path::Path, thread, time::Duration};

use anyhow::{Context, Result};
use parking_lot::Mutex;

use crate::{
    agent::claude,
    folder,
    map::{self, Map, Snapshot},
    pane::{self, Panes, login_environment},
};

/// How long a read waits for a pane's output before answering with none.
const READ_WAIT: Duration = Duration::from_secs(20);

pub struct Deck {
    map: Mutex<Map>,
    panes: Panes,
}

impl Deck {
    /// Opens the store at `store`, or the per-user one, beside Claude Code's
    /// own sessions.
    pub fn open(store: Option<&Path>) -> Result<Self> {
        // Asking the login shell for its environment can take a second, so it
        // is asked now and not when the first pane opens.
        thread::spawn(login_environment);
        // So is reading every installed font, for the Appearance sheet.
        thread::spawn(crate::fonts::installed);
        Ok(Self { map: Mutex::new(Map::at(store)?), panes: Panes::default() })
    }

    /// Answers a request about the map.
    pub fn map(&self, request: map::Request) -> Result<Snapshot> {
        self.map.lock().handle(request)
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
                    claude::for_pane(claude::open(session)?, light)
                };
                reply.pane = Some(self.panes.open(&launch, login_environment(), rows, cols)?);
            }
            pane::Request::Start { domain, folder, name, rows, cols, light } => {
                let (key, launch) = self.map.lock().plan_start(domain, &folder, name.as_deref())?;
                let launch = claude::for_pane(launch, light);
                let pane = self.panes.open(&launch, login_environment(), rows, cols)?;
                let recorded = self.map.lock().record_start(
                    domain,
                    key.clone(),
                    &launch.folder,
                    pane,
                    name.as_deref(),
                );
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
