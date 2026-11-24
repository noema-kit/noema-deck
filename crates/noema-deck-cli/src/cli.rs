//! The commands and their arguments.

use std::path::PathBuf;

use clap::{Parser, Subcommand};
use noema_deck_core::import;

#[derive(Parser)]
#[command(
    name = "noema-deck",
    version,
    about = "Arrange Claude Code sessions on a universe of topic stars"
)]
pub struct Cli {
    /// Store file. Defaults to the per-user application data directory.
    #[arg(long, global = true, value_name = "PATH")]
    pub store: Option<PathBuf>,
    #[command(subcommand)]
    pub command: Option<Command>,
}

#[derive(Subcommand)]
pub enum Command {
    /// List sessions, most recent first.
    Sessions {
        /// Only sessions on no star.
        #[arg(long)]
        unsorted: bool,
        /// Only sessions whose title or folder contains this text.
        #[arg(long, value_name = "TEXT")]
        query: Option<String>,
    },
    /// Open a session in Claude Code.
    Open {
        /// The session's ID, or the start of it.
        session: String,
    },
    /// Put a session on a star.
    Place {
        /// The session's ID, or the start of it.
        session: String,
        /// The star's ID or name.
        star: String,
    },
    /// Take a session off a star.
    Unplace {
        /// The session's ID, or the start of it.
        session: String,
        /// The star's ID or name.
        star: String,
    },
    /// Edit and inspect stars.
    #[command(subcommand)]
    Star(StarCommand),
    /// Serve the map to a browser on this machine.
    Serve {
        #[arg(long, default_value_t = 4270)]
        port: u16,
        /// Folder holding the interface files.
        #[arg(long, default_value = "ui", value_name = "DIR")]
        ui: PathBuf,
    },
}

#[derive(Subcommand)]
pub enum StarCommand {
    /// Show every star beneath its larger stars, with its session count.
    List,
    /// Add a top star, or a smaller star under another.
    Add {
        name: String,
        /// Hang the new star under this star.
        #[arg(long, value_name = "STAR")]
        under: Option<String>,
    },
    /// Add a folder, and the folders inside it, as a tree of stars.
    Import {
        /// The folder whose own star heads the tree.
        folder: PathBuf,
        /// Hang the folder's star under this star.
        #[arg(long, value_name = "STAR")]
        under: Option<String>,
        /// How many levels of folders inside it to add.
        #[arg(long, default_value_t = import::DEPTH)]
        depth: usize,
    },
    /// Rename a star.
    Rename { star: String, name: String },
    /// Delete a star. Its sessions stay with Claude Code.
    Delete { star: String },
    /// Hang a smaller star under a larger one.
    Link { larger: String, smaller: String },
    /// Remove the link between a larger and a smaller star.
    Unlink { larger: String, smaller: String },
    /// List the sessions on a star.
    Show {
        star: String,
        /// Include the stars beneath it.
        #[arg(long)]
        all: bool,
    },
}
