//! `noema-deck`: the desktop window. Given a command, it is the command-line
//! tool instead, so one program serves both.
//!
//! The interface in `ui/` reaches the core through the two commands below.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
#![expect(clippy::needless_pass_by_value, reason = "Tauri hands a command its arguments by value")]

use std::env;

use anyhow::Result;
use clap::Parser;
use noema_deck_cli::Cli;
use noema_deck_core::{
    Deck, Snapshot,
    map::Request as MapRequest,
    pane::{Reply as PaneReply, Request as PaneRequest},
};
use tauri::{AppHandle, State, webview::PageLoadEvent};

// Both commands run off the main thread: a map request may read every session
// from disk, and a pane read waits for output.

#[tauri::command(async)]
fn map(deck: State<'_, Deck>, request: MapRequest) -> Result<Snapshot, String> {
    deck.map(request).map_err(|error| format!("{error:#}"))
}

#[tauri::command(async)]
fn pane(deck: State<'_, Deck>, request: PaneRequest) -> Result<PaneReply, String> {
    deck.pane(request).map_err(|error| format!("{error:#}"))
}

/// Ends a self-test: prints what the interface reported and exits.
#[tauri::command]
fn selftest_done(app: AppHandle, report: String, failed: bool) {
    println!("{report}");
    app.exit(i32::from(failed));
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    if let Some(command) = cli.command {
        return noema_deck_cli::run(cli.store.as_deref(), command);
    }

    let deck = Deck::open(cli.store.as_deref())?;
    // `scripts/selftest.sh` sets this, along with fixtures for everything the
    // app reads.
    let selftest = env::var("NOEMA_DECK_SELFTEST").ok();
    tauri::Builder::default()
        .manage(deck)
        .on_page_load(move |webview, payload| {
            if let (Some(mode), PageLoadEvent::Finished) = (&selftest, payload.event()) {
                let hold = mode == "hold";
                let script = format!("import('./selftest.js').then((test) => test.run({hold}))");
                let _ = webview.eval(script);
            }
        })
        .invoke_handler(tauri::generate_handler![map, pane, selftest_done])
        .run(tauri::generate_context!())?;
    Ok(())
}
