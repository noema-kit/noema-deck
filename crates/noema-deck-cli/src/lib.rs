//! Noema Deck from the command line: list sessions, edit stars, place sessions
//! on them, and serve the map to a browser.
//!
//! The desktop app parses the same [`Cli`] and hands any command to [`run`], so
//! one program is both the window and the command-line tool.

mod cli;
mod find;
mod print;
mod serve;
mod sessions;
mod stars;

use std::path::Path;

use anyhow::Result;
use noema_deck_core::{Deck, Universe};

pub use self::cli::{Cli, Command, StarCommand};

/// Carries out one command against the store at `store`, or the per-user one.
pub fn run(store: Option<&Path>, command: Command) -> Result<()> {
    let universe = || Universe::at(store);
    match command {
        Command::Sessions { unsorted, query } => {
            sessions::list(&universe()?, unsorted, query.as_deref())
        }
        Command::Open { session } => sessions::open(&universe()?, &session),
        Command::Place { session, star } => sessions::place(&mut universe()?, &session, &star),
        Command::Unplace { session, star } => sessions::unplace(&mut universe()?, &session, &star),
        Command::Star(command) => stars::run(&mut universe()?, command),
        Command::Serve { port, ui } => serve::run(Deck::open(store)?, &ui, port),
    }
}
