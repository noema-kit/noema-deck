#!/bin/sh
# Runs the interface's self-test inside the real desktop window. The app gets a
# fixture store, three fixture sessions, a small folder tree, a stand-in for
# Claude Code and a plain shell, so nothing of yours is read or changed. Pass
# `hold` to keep a pane open for fifteen seconds partway through.
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

mkdir -p "$work/topics/os/kernel" "$work/topics/os/linux" "$work/topics/memory" "$work/topics/.hidden"

cat > "$work/agent" <<'EOF'
#!/bin/sh
echo "fake agent $*"
while read -r line; do echo "got $line"; done
EOF
chmod +x "$work/agent"

CLAUDE_CONFIG_DIR="$work/claude" \
NOEMA_DECK_STORE="$work/deck.db" \
NOEMA_DECK_CLAUDE="$work/agent" \
SHELL=/bin/sh \
NOEMA_DECK_SELFTEST="${1:-run}" \
    cargo run --quiet -p noema-deck
