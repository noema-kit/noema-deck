//! `noema-deck-cli`: Noema Deck without the window.

use anyhow::Result;
use clap::{CommandFactory, Parser};
use noema_deck_cli::Cli;

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Some(command) => noema_deck_cli::run(cli.store.as_deref(), command),
        None => Ok(Cli::command().print_help()?),
    }
}
