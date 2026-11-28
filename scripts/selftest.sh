#!/bin/sh
# Runs the interface's self-test. By default `noema-deck-cli serve` serves the
# interface to a headless Chrome, so no window opens; set `CHROME` to pick the
# browser. Pass `window` to run it in the real desktop window instead, or
# `hold` to do that and keep a pane open for fifteen seconds partway through.
# Deck gets a fixture store, three fixture Claude Code sessions, one Codex
# session and one Antigravity conversation, each agent with a reply's tokens
# and its plan's limits, a small folder tree, stand-ins for the agents and a
# plain shell, so nothing of yours is read or changed.
set -eu
cd "$(dirname "$0")/.."
mode=${1:-headless}

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

sessions="$work/claude/projects/fixture"
mkdir -p "$sessions"
number=1
for title in Alpha Beta Gamma; do
    printf '{"type":"ai-title","aiTitle":"%s","cwd":"%s"}\n' "$title" "$work" \
        > "$sessions/0000000$number-0000-4000-8000-000000000000.jsonl"
    number=$((number + 1))
done
now=$(date -u +%Y-%m-%dT%H:%M:%SZ)
# Claude Code's last answer to `/usage`: 8% of the session used.
printf '{"cachedUsageUtilization":{"fetchedAtMs":%s000,"utilization":{"limits":[{"kind":"session","percent":8,"resets_at":"2100-01-01T00:00:00Z"}]}}}\n' "$(date +%s)" \
    > "$work/claude/.claude.json"
# A reply in Alpha that took 1,000 tokens.
printf '{"type":"assistant","sessionId":"00000001-0000-4000-8000-000000000000","timestamp":"%s","message":{"id":"msg_1","model":"claude-opus-5-5","stop_reason":"end_turn","usage":{"input_tokens":100,"cache_read_input_tokens":800,"cache_creation_input_tokens":50,"output_tokens":50}}}\n' "$now" \
    >> "$sessions/00000001-0000-4000-8000-000000000000.jsonl"

# What was said in Beta, which a search reads.
printf '%s\n' '{"type":"user","message":{"role":"user","content":"Why does the journal replay twice?"}}' \
    '{"type":"assistant","message":{"content":[{"type":"text","text":"It is replayed once per mount."}]}}' \
    >> "$sessions/00000002-0000-4000-8000-000000000000.jsonl"

# A Codex rollout: what the session is and where it ran, then its first prompt.
day="$work/codex/sessions/2026/11/26"
mkdir -p "$day"
cat > "$work/rollout" <<EOF
#!/bin/sh
{
    printf '{"type":"session_meta","payload":{"id":"%s","cwd":"%s","source":"cli"}}\\n' "\$1" "\$2"
    printf '{"type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"%s"}]}}\\n' "\$3"
} > "$day/rollout-2026-11-26T10-00-00-\$1.jsonl"
EOF
chmod +x "$work/rollout"
"$work/rollout" 01a20000-0000-7000-8000-000000000001 "$work" Delta
# A reply in Delta that took 2,000 tokens, with a quarter of the week's allowance used.
printf '{"timestamp":"%s","type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":1500,"cached_input_tokens":1000,"output_tokens":500,"total_tokens":2000},"last_token_usage":{"input_tokens":1500,"cached_input_tokens":1000,"output_tokens":500,"total_tokens":2000}},"rate_limits":{"primary":{"used_percent":25.0,"window_minutes":10080,"resets_at":4102444800},"secondary":null}}}\n' "$now" \
    >> "$day/rollout-2026-11-26T10-00-00-01a20000-0000-7000-8000-000000000001.jsonl"

# What Codex answered in Delta.
printf '%s\n' '{"type":"response_item","payload":{"type":"message","role":"assistant","content":[{"type":"output_text","text":"Journal written to disk."}]}}' \
    >> "$day/rollout-2026-11-26T10-00-00-01a20000-0000-7000-8000-000000000001.jsonl"

# An Antigravity conversation: its summary, where `agy` lists it, and its file.
# As in `agy`, a new conversation has no title, only its first message.
agy="$work/antigravity"
mkdir -p "$agy/conversations"
sqlite3 "$agy/conversation_summaries.db" "CREATE TABLE conversation_summaries (
    conversation_id text PRIMARY KEY, title text NOT NULL DEFAULT '',
    preview text NOT NULL DEFAULT '', step_count integer NOT NULL DEFAULT 0,
    last_modified_time datetime NOT NULL, workspace_uris text NOT NULL,
    parent_conversation_id text NOT NULL DEFAULT '',
    nesting_depth integer NOT NULL DEFAULT 0)"
cat > "$work/conversation" <<EOF
#!/bin/sh
sqlite3 "$agy/conversation_summaries.db" "INSERT INTO conversation_summaries
    (conversation_id, preview, step_count, last_modified_time, workspace_uris)
    VALUES ('\$1', '\$3', 1, '2026-11-27 10:00:00+00:00', '[\"file://\$2\"]')"
: > "$agy/conversations/\$1.db"
EOF
chmod +x "$work/conversation"
"$work/conversation" 6b4c3a20-0000-4000-8000-000000000001 "$work" Epsilon
# A reply in Epsilon that took 4,000 tokens.
logs="$agy/brain/6b4c3a20-0000-4000-8000-000000000001/.system_generated/logs"
mkdir -p "$logs"
printf '{"step_index":1,"type":"PLANNER_RESPONSE","status":"DONE","created_at":"%s","model":"gemini-4-neon","input_tokens":3000,"output_tokens":1000}\n' "$now" \
    > "$logs/transcript_full.jsonl"

mkdir -p "$work/topics/os/kernel" "$work/topics/os/linux" "$work/topics/memory" "$work/topics/.hidden"

cat > "$work/agent" <<'EOF'
#!/bin/sh
echo "fake agent $*"
while read -r line; do echo "got $line"; done
EOF
chmod +x "$work/agent"

# Codex chooses a new session's ID itself. Its stand-in stores one at once.
cat > "$work/codex-agent" <<EOF
#!/bin/sh
[ \$# -eq 0 ] && "$work/rollout" 01a20000-0000-7000-8000-00000000000a "\$(pwd -P)" Hello
echo "fake codex \$*"
while read -r line; do echo "got \$line"; done
EOF
chmod +x "$work/codex-agent"

# So does `agy`. Its stand-in also stores one at once, and answers `/usage`
# with a quarter of the Gemini models' week used.
cat > "$work/agy-usage.json" <<'EOF'
{"status":"SUCCESS","command":{"data":{"groups":[{"name":"Gemini Models","buckets":[{"window":"weekly","remaining_fraction":0.75,"reset_time":"2100-01-01T00:00:00Z"}]}]}}}
EOF
cat > "$work/agy-agent" <<EOF
#!/bin/sh
[ "\$1" = --print ] && exec cat "$work/agy-usage.json"
[ \$# -eq 0 ] && "$work/conversation" 6b4c3a20-0000-4000-8000-00000000000a "\$(pwd -P)" Hello
echo "fake agy \$*"
while read -r line; do echo "got \$line"; done
EOF
chmod +x "$work/agy-agent"

export CLAUDE_CONFIG_DIR="$work/claude"
export CODEX_HOME="$work/codex"
export NOEMA_DECK_STORE="$work/deck.db"
export NOEMA_DECK_CLAUDE="$work/agent"
export NOEMA_DECK_CODEX="$work/codex-agent"
export NOEMA_DECK_ANTIGRAVITY_HOME="$agy"
export NOEMA_DECK_ANTIGRAVITY="$work/agy-agent"
export SHELL=/bin/sh
export NOEMA_DECK_SELFTEST="$mode"

if [ "$mode" != headless ]; then
    cargo run --quiet -p noema-deck
    exit
fi

chrome=${CHROME:-}
for candidate in \
    "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome" \
    "/Applications/Chromium.app/Contents/MacOS/Chromium" \
    "/Applications/Microsoft Edge.app/Contents/MacOS/Microsoft Edge" \
    "$(command -v google-chrome chromium chromium-browser | head -n 1)"; do
    if [ -z "$chrome" ] && [ -x "$candidate" ]; then chrome=$candidate; fi
done
if [ -z "$chrome" ]; then
    echo "No Chrome to run the self-test in: set CHROME, or pass \`window\`" >&2
    exit 1
fi

cargo build --quiet -p noema-deck-cli
# The server takes any free port and names it on its first line. A page
# that goes away breaks the reads it was waiting on, which the server notes;
# those notes are shown only when the run fails.
target/debug/noema-deck-cli serve --ui ui --port 0 > "$work/report" 2> "$work/notes" &
server=$!
until url=$(grep -o 'http://[^ ]*' "$work/report"); do
    kill -0 "$server" 2> /dev/null || exit 1
    sleep 0.1
done
"$chrome" --headless=new --user-data-dir="$work/chrome" --window-size=1280,820 \
    --no-first-run --no-default-browser-check "$url" > /dev/null 2>&1 &
browser=$!

# The page reports when every step has run, and the server then exits.
waited=0
while kill -0 "$server" 2> /dev/null; do
    if [ "$waited" -ge 300 ]; then
        echo "The self-test did not finish in five minutes" >&2
        kill "$server"
    fi
    sleep 1
    waited=$((waited + 1))
done
status=0
wait "$server" || status=$?
kill "$browser" 2> /dev/null || true
wait "$browser" 2> /dev/null || true
sed 1d "$work/report"
[ "$status" -eq 0 ] || cat "$work/notes" >&2
exit "$status"
