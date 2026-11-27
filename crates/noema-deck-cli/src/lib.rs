//! Noema Deck from the command line: list sessions, edit domains, assign sessions
//! on them, count the tokens used, and serve the map to a browser.
//!
//! The desktop app parses the same [`Cli`] and hands any command to [`run`], so
//! one program is both the window and the command-line tool.

mod cli;
mod domains;
mod find;
mod print;
mod serve;
mod sessions;
mod usage;

use std::path::Path;

use anyhow::Result;
use noema_deck_core::{Deck, Map};

pub use self::cli::{Cli, Command, DomainCommand};

/// Carries out one command against the store at `store`, or the per-user one.
pub fn run(store: Option<&Path>, command: Command) -> Result<()> {
    let map = || Map::at(store);
    match command {
        Command::Sessions { unassigned, query } => {
            sessions::list(&map()?, unassigned, query.as_deref())
        }
        Command::Open { session } => sessions::open(&map()?, &session),
        Command::Rename { session, title } => sessions::rename(&mut map()?, &session, &title),
        Command::Assign { session, domain } => sessions::assign(&mut map()?, &session, &domain),
        Command::Unassign { session, domain } => sessions::unassign(&mut map()?, &session, &domain),
        Command::Usage { limits: true, .. } => usage::limits(),
        Command::Usage { days, .. } => usage::print(days),
        Command::Domain(command) => domains::run(&mut map()?, command),
        Command::Serve { port, ui } => serve::run(Deck::open(store)?, &ui, port),
    }
}
