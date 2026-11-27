#!/bin/sh
# Runs the interface's self-test inside the real desktop window. The app gets a
# fixture store, three fixture Claude Code sessions, one Codex session and one
# Antigravity conversation, a small folder tree, stand-ins for the agents and a
# plain shell, so nothing of yours is read or changed. Pass `hold` to keep a pane open for fifteen seconds
# partway through.
set -eu
cd "$(dirname "$0")/.."

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

# An Antigravity conversation: its summary, where `agy` lists it, and its file.
agy="$work/antigravity"
mkdir -p "$agy/conversations"
sqlite3 "$agy/conversation_summaries.db" "CREATE TABLE conversation_summaries (
    conversation_id text PRIMARY KEY, title text NOT NULL DEFAULT '',
    step_count integer NOT NULL DEFAULT 0, last_modified_time datetime NOT NULL,
    workspace_uris text NOT NULL, parent_conversation_id text NOT NULL DEFAULT '',
    nesting_depth integer NOT NULL DEFAULT 0)"
cat > "$work/conversation" <<EOF
#!/bin/sh
sqlite3 "$agy/conversation_summaries.db" "INSERT INTO conversation_summaries
    (conversation_id, title, step_count, last_modified_time, workspace_uris)
    VALUES ('\$1', '\$3', 1, '2026-11-27 10:00:00+00:00', '[\"file://\$2\"]')"
: > "$agy/conversations/\$1.db"
EOF
chmod +x "$work/conversation"
"$work/conversation" 6b4c3a20-0000-4000-8000-000000000001 "$work" Epsilon

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

# So does `agy`. Its stand-in also stores one at once.
cat > "$work/agy-agent" <<EOF
#!/bin/sh
[ \$# -eq 0 ] && "$work/conversation" 6b4c3a20-0000-4000-8000-00000000000a "\$(pwd -P)" Hello
echo "fake agy \$*"
while read -r line; do echo "got \$line"; done
EOF
chmod +x "$work/agy-agent"

CLAUDE_CONFIG_DIR="$work/claude" \
CODEX_HOME="$work/codex" \
NOEMA_DECK_STORE="$work/deck.db" \
NOEMA_DECK_CLAUDE="$work/agent" \
NOEMA_DECK_CODEX="$work/codex-agent" \
NOEMA_DECK_ANTIGRAVITY_HOME="$agy" \
NOEMA_DECK_ANTIGRAVITY="$work/agy-agent" \
SHELL=/bin/sh \
NOEMA_DECK_SELFTEST="${1:-run}" \
    cargo run --quiet -p noema-deck
