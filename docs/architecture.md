# Architecture

How Deck is built. [Design](design.md) says what it does.

## Crates

```mermaid
flowchart LR
    UI["ui/: map, lists and terminals"] -->|map, pane| APP["noema-deck: desktop window"]
    UI -->|/map, /pane| CLI["noema-deck-cli: commands and local server"]
    APP --> CLI
    APP --> CORE["noema-deck-core"]
    CLI --> CORE
    CORE --> S["SQLite store"]
    CORE --> A["Claude Code, Codex and Antigravity: session files and CLIs"]
    CORE --> T["Pseudoterminals"]
```

| Part | Role |
| --- | --- |
| `noema-deck-core` | Everything Deck knows and does. No window, no arguments, no printing. |
| `noema-deck-cli` | The commands, what they print, and a local server that gives the interface to a browser. |
| `noema-deck` | The Tauri window. Given a command, it hands it to the CLI library, so one program is both the app and the command-line tool. |
| `ui/` | Plain HTML, CSS and JavaScript with no build step. `app.js` sends requests and fills the lists, `map.js` runs the map and hands its drawing to the theme's painter, `theme.js` and `settings.js` put on and change the look, and `keys.js` holds the shortcuts. Each theme is a folder in `ui/themes/`. |

## Core modules

| Module | Holds |
| --- | --- |
| `agent`, `agent::claude`, `agent::codex`, `agent::antigravity` | The agents, and one adapter each: list and rename sessions, and the commands that open or start one. |
| `session`, `domain` | The records Deck works with. |
| `store` | The SQLite file: tables and upgrades, domains, assignments, settings. |
| `import` | A folder tree turned into domains laid out on rings. |
| `map` | Domains, links and assignments joined with the agents' sessions. One `Request` goes in and one `Snapshot` comes out. `map::arrange` lays the domains out by depth around one centre. |
| `pane` | Terminal panes on pseudoterminals. |
| `fonts` | The installed typefaces, read once: which share one width, and which draw prompt icons. |
| `deck` | One running Deck: the map behind a lock, and the panes. |

## One request, one snapshot

The interface keeps no state of its own about domains or sessions. It sends a request and draws the snapshot that comes back.

```mermaid
sequenceDiagram
    participant UI as Interface
    participant D as Deck
    participant M as Map
    UI->>D: map { command: "link", args }
    D->>M: handle(request)
    M->>M: change the store
    M-->>UI: Snapshot: domains, links, sessions, missing
```

Every rule lives in the core: a refused link, a domain's count, which sessions are missing. The window and the server pass the same routes through unchanged, `map`, `pane` and `fonts`, so the interface behaves the same in both. A snapshot equal to the one on screen is not drawn again.

## Interface

- **Local storage** holds only layout choices, such as panel widths and folded domains. The panes to reopen, the appearance and the shortcuts go to the store, so they follow the store and not the web view.
- **Look.** The appearance comes in the snapshot and is changed by a request, like everything else. `theme.js` loads the theme's style sheet and painter, sets the mode and typefaces on the page, and restyles the map and every terminal. `boot.js` runs before the page is drawn and puts on the last run's look from local storage, so the window does not flash the default.
- **Title bar.** The window's title bar is an overlay, with its buttons centred on the page's top row. The page marks the regions that drag and zoom the window. One capability file grants it those two calls and the app's version, for Settings.

## Map drawing

The map is one canvas, because a pan or a zoom repaints every domain on every frame.

- **View and painter.** `map.js` owns the view, its motion, hit testing, names and counts. The theme's painter draws the backdrop, links and domains. [Themes](themes.md#the-painter) gives the interface.
- **A frame only when something changed.** A still map costs nothing.
- **Hit testing is arithmetic.** The domain under the pointer is the nearest one within reach. No element stands for one.

## Store

One SQLite file in the per-user application data directory. Nothing from a session's content is stored.

| Table | Columns |
| --- | --- |
| `domain` | `id`, `name`, `x`, `y` |
| `link` | `parent`, `child` |
| `assignment` | `domain`, `agent`, `session` |
| `setting` | `name`, `value`: the last folder, the panes to reopen with their layout tree, the appearance, and the shortcuts moved to other keys |

`PRAGMA user_version` counts the upgrade steps a store has had. Opening runs the ones it lacks, and refuses a store from a newer Deck.

## Panes

A pane runs one command on a pseudoterminal: an agent on a session, or the user's login shell.

- **Two threads per pane.** One copies what the process writes. The other waits for the process to end.
- **Ending.** A pane ends when its terminal closes, or shortly after its process exits if something it started holds the terminal open.
- **Many panes.** Any number can run. The interface keeps reading those out of sight too, which is how it knows a session is active.
- **Reading waits.** The interface asks for output and the answer comes when there is some, or after twenty seconds.
- **Environment.** A desktop app starts without a terminal's `PATH`. Deck asks the login shell for its environment once, at startup, and runs every pane with it, minus the variables of whatever terminal started Deck, and with `TERM=xterm-256color`, `COLORTERM=truecolor` and `TERM_PROGRAM=NoemaDeck`.
- **Drawing.** Each terminal is xterm.js drawing through WebGL, and through plain elements when WebGL is unavailable. The page answers a program's question about light and dark (mode 2031) itself.

## Local server

`noema-deck serve` gives the same interface to a browser. It listens on this machine only. Because a pane can type into an agent, it answers only its own page and only JSON requests, which a page from another site cannot send.

## Claude Code

| Action | Interface |
| --- | --- |
| List | Transcript files, `~/.claude/projects/<project>/<session-id>.jsonl` |
| Title | Last `custom-title` record, else `ai-title`, else `last-prompt`, else the first typed prompt or command |
| Rename | Append a `custom-title` record, as `/rename` does, and keep the file's modification time |
| Folder | `cwd` of the first record that has one |
| Last activity | File modification time |
| Open | `claude --resume <id>` in the session's folder |
| Start | `claude --session-id <uuid>` with an ID Deck chooses, and `--name` when named |
| Theme | In a pane, `--settings` swaps Claude Code's theme for its light or dark counterpart to match the pane, for that run only |

Read from Claude Code 2.1.330 on macOS. The transcript format is undocumented, so Deck reads only these fields, and reads a transcript again only when its time or length has changed.

## Codex

| Action | Interface |
| --- | --- |
| List | Rollout files, `~/.codex/sessions/<year>/<month>/<day>/rollout-<time>-<id>.jsonl`, or `.jsonl.zst` once Codex has compressed them. Only sessions from the terminal, editor and desktop apps, as `codex resume` lists them, not sub-agents or non-interactive runs |
| Title | Last name for the session in `~/.codex/session_index.jsonl`, else the first prompt typed. A session with neither is empty and left out |
| Rename | Append a name to `session_index.jsonl`, where `codex resume` also finds sessions by name |
| Folder | `cwd` of the first record, `session_meta` |
| Last activity | File modification time |
| Open | `codex resume <id>` in the session's folder |
| Start | `codex` in the folder. Codex chooses the ID, so Deck holds the session under a stand-in key and takes Codex's ID once Codex lists a new session in that folder, moving its domains and its kept pane along. A name given is written once Codex has stored it |

Read from Codex 0.171.0 on macOS. `CODEX_HOME` moves the folder, as it does for Codex.

## Antigravity

| Action | Interface |
| --- | --- |
| List | Conversation summaries in the SQLite file `~/.gemini/antigravity-cli/conversation_summaries.db`, read only. Only conversations with a step and a file in `conversations/`, as `agy`'s `/resume` lists them, not sub-agents |
| Title | `title` in the conversation's annotations, `annotations/<id>.pbtxt`, else the summary's title, else the first prompt typed in `history.jsonl`. A conversation with none is left out |
| Rename | Set `title` in the annotations, where `agy` keeps the names given to conversations and copies them into its summaries |
| Folder | First `file://` URI in the summary's `workspace_uris` |
| Last activity | The later of the summary's time and the conversation file's modification time |
| Open | `agy --conversation <id>` in the conversation's folder |
| Start | `agy` in the folder. `agy` chooses the ID, so Deck follows the first new conversation in that folder, as for Codex |
| Theme | In a pane, `--settings` swaps a light or dark `colorScheme` for its counterpart to match the pane, for that run only |

Read from the Antigravity CLI, `agy` 1.5.18, on macOS. `agy` always uses that folder. `NOEMA_DECK_ANTIGRAVITY_HOME` points Deck elsewhere, for tests.
