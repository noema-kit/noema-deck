<p align="center">
  <img src="crates/noema-deck/icons/icon.png" width="128" alt="Noema Deck icon">
</p>

<h1 align="center">Noema Deck</h1>

<p align="center">A desktop workbench that organises Claude Code sessions by domain.</p>

![The map of domains, with the session list on the left and the System domain's panel on the right](docs/images/map.jpg)

Deck is the first part of [Noema Kit](https://github.com/noema-kit), a set of tools for building an AI-native personal knowledge system.

## Features

- **Domains.** Group sessions by domain on a map. A domain can have subdomains and several parents, and a session can belong to several domains.
- **Original sessions.** Deck manages the sessions Claude Code creates. It stores only assignments and keeps no copy.
- **Opt-in.** A session appears once it is assigned to a domain. Start one in a domain, or add past ones from the history.
- **Terminal.** A session opens in a terminal pane that runs Claude Code. Panes split right or down, and a pane can run a plain shell.
- **Command line.** The same program lists, opens and assigns sessions from a shell.

## Run

Requires Rust 1.95 and Claude Code.

```sh
cargo run -p noema-deck         # the desktop window
cargo run -- serve              # the same interface at http://127.0.0.1:4270
cargo run -- --help             # the commands
```

## Use

| Action | How |
| --- | --- |
| Add a domain | Double-click the map. A folder path imports its tree. |
| Move or link domains | Drag one. Shift-drag from a parent to a subdomain. |
| Start a session | **New session** in the domain panel, or `+` in the session list |
| Add past sessions | **Add existing** in the domain panel |
| Open a session | Click it. The split button on its row opens it beside the current pane. |
| Open a shell | `>_` in the session list, or split a pane |
| Resize | Drag the edge of the session list or the domain panel, or the divider between panes |

| Key | Action |
| --- | --- |
| ⌘1, ⌘2 | Map, terminal |
| ⌘B, ⌘K | Session list, search |
| ⌘D, ⇧⌘D | Split right, split down |

Off macOS, the number keys take Ctrl and the letter keys take Ctrl+Shift.

## Documents

| Document | Content |
| --- | --- |
| [Design](docs/design.md) | What Deck does. |
| [Architecture](docs/architecture.md) | How it is built. |
| [Development](docs/development.md) | Running, checking and conventions. |

## Licence

[MIT](LICENSE)
