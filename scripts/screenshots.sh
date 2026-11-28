#!/bin/sh
# Retakes the screenshots in docs/images. It builds a made-up world, serves
# it, and drives a headless Chrome through Deck's own interface, so nothing of
# yours shows. Needs python3, node and Chrome; set CHROME to pick another
# Chromium browser.
set -eu
cd "$(dirname "$0")/.."
work=$(mktemp -d)
cli=target/debug/noema-deck-cli
cargo build --quiet -p noema-deck-cli
python3 scripts/screenshots/world.py "$work" "$cli"

env -i HOME="$work/home" PATH=/usr/bin:/bin LANG=en_US.UTF-8 SHELL=/bin/zsh ZDOTDIR="$work/home/.zdotdir" \
    CLAUDE_CONFIG_DIR="$work/home/.claude" CODEX_HOME="$work/home/.codex" \
    NOEMA_DECK_ANTIGRAVITY_HOME="$work/home/.gemini/antigravity-cli" \
    NOEMA_DECK_CLAUDE="$work/agent" NOEMA_DECK_CODEX="$work/agent" NOEMA_DECK_ANTIGRAVITY="$work/agy-agent" \
    "$cli" --store "$work/deck.db" serve --ui ui --port 0 > "$work/served" 2> /dev/null &
server=$!
"${CHROME:-/Applications/Google Chrome.app/Contents/MacOS/Google Chrome}" --headless=new \
    --remote-debugging-port=0 --user-data-dir="$work/chrome" --no-first-run --no-default-browser-check \
    about:blank > /dev/null 2>&1 &
browser=$!
trap 'kill "$server" "$browser" 2> /dev/null; wait "$browser" 2> /dev/null; rm -rf "$work"' EXIT

# Both take any free port and say which.
waited=0
until url=$(grep -o 'http://[^ ]*' "$work/served") && [ -s "$work/chrome/DevToolsActivePort" ]; do
    [ "$waited" -lt 100 ] || { echo "The server or Chrome did not start" >&2; exit 1; }
    sleep 0.1
    waited=$((waited + 1))
done
mkdir "$work/out"
node scripts/screenshots/shoot.mjs "$(head -n 1 "$work/chrome/DevToolsActivePort")" "$url" "$work/out"
for shot in "$work"/out/*.jpg; do
    sips -Z 1800 -s formatOptions 85 "$shot" --out "docs/images/$(basename "$shot")" > /dev/null
done
