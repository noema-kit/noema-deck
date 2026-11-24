//! The commands and their arguments.

use std::path::PathBuf;

use clap::{Parser, Subcommand};
use noema_deck_core::import;

#[derive(Parser)]
#[command(name = "noema-deck", version, about = "Organise Claude Code sessions by domain")]
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
        /// Only sessions assigned to no domain.
        #[arg(long)]
        unassigned: bool,
        /// Only sessions whose title or folder contains this text.
        #[arg(long, value_name = "TEXT")]
        query: Option<String>,
    },
    /// Open a session in Claude Code.
    Open {
        /// The session's ID, or the start of it.
        session: String,
    },
    /// Name a session, as Claude Code's `/rename` does.
    Rename {
        /// The session's ID, or the start of it.
        session: String,
        title: String,
    },
    /// Assign a session to a domain.
    Assign {
        /// The session's ID, or the start of it.
        session: String,
        /// The domain's ID or name.
        domain: String,
    },
    /// Remove a session from a domain.
    Unassign {
        /// The session's ID, or the start of it.
        session: String,
        /// The domain's ID or name.
        domain: String,
    },
    /// Edit and inspect domains.
    #[command(subcommand)]
    Domain(DomainCommand),
    /// Serve the interface to a browser on this machine.
    Serve {
        #[arg(long, default_value_t = 4270)]
        port: u16,
        /// Folder holding the interface files.
        #[arg(long, default_value = "ui", value_name = "DIR")]
        ui: PathBuf,
    },
}

#[derive(Subcommand)]
pub enum DomainCommand {
    /// Show the domain tree with session counts.
    List,
    /// Add a root domain, or a subdomain under another.
    Add {
        name: String,
        /// The parent domain.
        #[arg(long, value_name = "STAR")]
        under: Option<String>,
    },
    /// Add a folder, and the folders inside it, as a tree of domains.
    Import {
        /// The folder whose own domain heads the tree.
        folder: PathBuf,
        /// The parent domain.
        #[arg(long, value_name = "STAR")]
        under: Option<String>,
        /// How many levels of folders inside it to add.
        #[arg(long, default_value_t = import::DEPTH)]
        depth: usize,
    },
    /// Rename a domain.
    Rename { domain: String, name: String },
    /// Delete a domain. Its sessions stay with Claude Code.
    Delete { domain: String },
    /// Make one domain a subdomain of another.
    Link { parent: String, child: String },
    /// Remove the link between a parent and a subdomain.
    Unlink { parent: String, child: String },
    /// List the sessions in a domain.
    Show {
        domain: String,
        /// Include the subdomains.
        #[arg(long)]
        all: bool,
    },
}
