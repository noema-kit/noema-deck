# Development

## Run

Requires Rust 1.95, which `rust-toolchain.toml` selects, and Claude Code sessions on this machine.

```sh
cargo run -p noema-deck         # the desktop window
cargo run -- serve              # the same map at http://127.0.0.1:4270
cargo run -- sessions           # any command, without the window
```

Plain `cargo` commands leave the window out, because it pulls in a web view. Name it with `-p noema-deck`, or use `--workspace`. The first build takes a few minutes, because development builds optimise the core and every dependency.

| Variable | Effect |
| --- | --- |
| `NOEMA_DECK_STORE` | The store file, in place of the per-user one. `--store` does the same. |
| `NOEMA_DECK_CLAUDE` | The Claude Code executable, when it is not `claude` on the `PATH`. |
| `CLAUDE_CONFIG_DIR` | Where Claude Code keeps its sessions, as Claude Code itself reads it. |

## Check

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
scripts/selftest.sh
```

The self-test opens the real window and drives the map, the session list, terminal panes, a new session, an import and the Appearance sheet from inside the page. It uses a fixture store, fixture sessions and a stand-in for Claude Code, so it reads and changes nothing of yours.

## Conventions

- **Rules live in the core.** A new behaviour starts as a core function with a test. The interface only draws.
- **Errors** are `anyhow` results whose message can be shown to a person as it stands.
- **Comments** say why. A doc comment says what a caller needs to know.
- **Commits** take an imperative subject of at most 72 characters and hold one logical change.

## Vendored and generated files

| Files | Note |
| --- | --- |
| `ui/vendor/xterm*` | xterm.js with its fit and WebGL add-ons. [Source and licence](../ui/vendor/xterm.SOURCE.md) |
| `crates/noema-deck/icons/*.png`, `icon.icns` | Rendered from `icon.svg`, the Noema Kit mark, by `scripts/icons.sh`, which needs `rsvg-convert`. `ui/icon.svg` is the same mark with the margin cut away. |
