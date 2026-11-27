//! `noema-deck`: the desktop window. Given a command, it is the command-line
//! tool instead, so one program serves both.
//!
//! The interface in `ui/` reaches the core through the commands below.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
#![expect(clippy::needless_pass_by_value, reason = "Tauri hands a command its arguments by value")]

use std::{env, sync::Arc};

use anyhow::Result;
use clap::Parser;
use noema_deck_cli::Cli;
use noema_deck_core::{
    Deck, Snapshot,
    fonts::{self, Font},
    map::Request as MapRequest,
    pane::{Reply as PaneReply, Request as PaneRequest},
    usage::{LimitsRequest, Plan, Report},
};
use tauri::{AppHandle, State, webview::PageLoadEvent};

// Each command runs on a thread kept for work that waits: a map request may
// read every session from disk, a usage request every record of the agents, a
// limits request waits for the agents to answer, and a pane read waits up to
// twenty seconds for output. Run on the async runtime's few workers instead,
// a handful of open panes would leave every other request waiting in line.

#[tauri::command]
async fn map(deck: State<'_, Arc<Deck>>, request: MapRequest) -> Result<Snapshot, String> {
    waiting(&deck, move |deck| deck.map(request)).await
}

#[tauri::command]
async fn pane(deck: State<'_, Arc<Deck>>, request: PaneRequest) -> Result<PaneReply, String> {
    waiting(&deck, move |deck| deck.pane(request)).await
}

#[tauri::command]
async fn usage(deck: State<'_, Arc<Deck>>) -> Result<Report, String> {
    waiting(&deck, Deck::usage).await
}

/// Each agent's plan limits, or one agent's. With `ask`, the agents are asked first.
#[tauri::command]
async fn limits(deck: State<'_, Arc<Deck>>, request: LimitsRequest) -> Result<Vec<Plan>, String> {
    waiting(&deck, move |deck| deck.limits(request)).await
}

/// Runs `work` on the runtime's threads for blocking work.
async fn waiting<T: Send + 'static>(
    deck: &Arc<Deck>,
    work: impl FnOnce(&Deck) -> Result<T> + Send + 'static,
) -> Result<T, String> {
    let deck = Arc::clone(deck);
    let done = tauri::async_runtime::spawn_blocking(move || work(&deck)).await;
    done.map_err(|error| error.to_string())?.map_err(|error| format!("{error:#}"))
}

/// The installed typefaces, for Settings.
#[tauri::command(async)]
fn fonts() -> &'static [Font] {
    fonts::installed()
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
        .manage(Arc::new(deck))
        .on_page_load(move |webview, payload| {
            if let (Some(mode), PageLoadEvent::Finished) = (&selftest, payload.event()) {
                let hold = mode == "hold";
                let script = format!("import('./selftest.js').then((test) => test.run({hold}))");
                let _ = webview.eval(script);
            }
        })
        .invoke_handler(tauri::generate_handler![map, pane, usage, limits, fonts, selftest_done])
        .run(tauri::generate_context!())?;
    Ok(())
}
