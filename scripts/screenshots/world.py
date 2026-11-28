"""Builds a made-up world for the README's screenshots in the folder given:
a home with sessions of Claude Code, Codex and Antigravity, a year of token
use, plan limits and a small project, stand-ins for the agents, and a store
with domains. Usage: world.py FOLDER CLI"""
import datetime as dt
import json
import os
import pathlib
import random
import sqlite3
import subprocess
import sys
import time
import urllib.request

compact = lambda value: json.dumps(value, separators=(",", ":"), ensure_ascii=False)

here, cli = pathlib.Path(sys.argv[1]), sys.argv[2]
ui = pathlib.Path(__file__).resolve().parents[2] / "ui"
home, store = here / "home", here / "deck.db"
random.seed(7)
now = time.time()
iso = lambda at: dt.datetime.fromtimestamp(at, dt.timezone.utc).strftime("%Y-%m-%dT%H:%M:%S.000Z")

# Domains: name -> (x, y, parents)
domains = {
    "System": (0, 0, []), "AI": (640, -250, []), "PL": (560, 330, []),
    "Operating system": (-270, -170, ["System"]), "Kernel": (-480, -270, ["Operating system"]),
    "File systems": (-470, -70, ["Operating system"]), "Scheduling": (-250, -370, ["Operating system"]),
    "Storage": (-170, 240, ["System"]), "Networking": (-360, 130, ["System"]),
    "Memory": (190, 150, ["System"]), "CXL": (110, 330, ["Memory"]),
    "Agents": (450, -420, ["AI"]), "Tools": (270, -510, ["Agents"]),
    "Agent memory": (330, -130, ["Agents", "Memory"]),
    "Training": (850, -400, ["AI"]), "Inference": (880, -130, ["AI"]),
    "Rust": (780, 200, ["PL"]), "Async": (980, 110, ["Rust"]), "Ownership": (990, 300, ["Rust"]),
    "Compilers": (720, 510, ["PL"]), "Type systems": (410, 520, ["PL"]),
}
# title, agent, folder under home, days since last worked on, domain
sessions = [
    ("Trace a page fault through the kernel", "claude", "code/kernel", 0.05, "Kernel"),
    ("Why the scheduler picks this task", "codex", "code/kernel", 3, "Kernel"),
    ("Read the slab allocator", "antigravity", "code/kernel", 0.4, "Kernel"),
    ("Journal replay after a crash", "claude", "code/fs", 1, "File systems"),
    ("Extent trees against block maps", "codex", "code/fs", 20, "File systems"),
    ("Lag and deadlines in EEVDF", "claude", "notes/sched", 6, "Scheduling"),
    ("NVMe queue depth experiments", "codex", "bench/storage", 2, "Storage"),
    ("Write pointers on zoned storage", "claude", "bench/storage", 40, "Storage"),
    ("Zero-copy receive path", "claude", "code/net", 75, "Networking"),
    ("Compaction stalls with huge pages", "claude", "code/kernel", 0.2, "Memory"),
    ("Tiering policy for CXL memory", "antigravity", "notes/cxl", 9, "CXL"),
    ("Design a tool-calling loop", "codex", "code/agent", 0.1, "Agents"),
    ("Hand off between two agents", "claude", "code/agent", 0.6, "Agents"),
    ("Sandbox a shell tool", "codex", "code/agent", 4, "Tools"),
    ("Memory that survives a restart", "claude", "code/agent", 1, "Agent memory"),
    ("Why the loss plateaus early", "antigravity", "ml/train", 15, "Training"),
    ("Batch requests for throughput", "antigravity", "ml/serve", 30, "Inference"),
    ("Quantisation error by layer", "claude", "ml/serve", 88, "Inference"),
    ("Lifetimes in a parser", "claude", "code/parser", 0.3, "Rust"),
    ("Cancel safety in select", "codex", "code/runtime", 2, "Async"),
    ("Split a borrow across a tree", "claude", "code/parser", 12, "Ownership"),
    ("Walk through SSA construction", "claude", "code/compiler", 5, "Compilers"),
    ("Register allocation by colouring", "codex", "code/compiler", 50, "Compilers"),
    ("Bidirectional type checking", "antigravity", "notes/types", 25, "Type systems"),
    ("Draft the weekly review", "claude", "notes", 0.02, None),
    ("Explain this flame graph", "codex", "bench/storage", 0.8, None),
]
models = {"claude": ["claude-opus-5-5", "claude-sonnet-5-5"], "codex": ["gpt-6.2"], "antigravity": ["gemini-4-neon"]}
claude_dir, codex_dir = home / ".claude", home / ".codex"
agy_dir = home / ".gemini" / "antigravity-cli"
projects = claude_dir / "projects" / "fixture"
for folder in [projects, codex_dir / "sessions", agy_dir / "conversations"]:
    folder.mkdir(parents=True)

# One calendar for every agent: most weekends and two holidays off.
off = {age for age in range(365) if dt.date.fromtimestamp(now - age * 86400).weekday() >= 5 and random.random() < 0.8}
off |= set(range(44, 53)) | set(range(160, 171))

def replies(days_ago, scale):
    """Replies on the working days of the year before the session was last worked on, busier lately."""
    out = []
    for age in range(int(days_ago), 365):
        if age in off or random.random() < 0.35:
            continue
        for _ in range(random.randint(1, 3)):
            at = now - age * 86400 - random.uniform(0, 30000)
            out.append((at, int(scale * random.uniform(0.3, 1.6) * (1.5 - age / 500))))
    return sorted(out)

ids = {}
summaries = []
for number, (title, agent, folder, days, _) in enumerate(sessions, 1):
    cwd = home / folder
    cwd.mkdir(parents=True, exist_ok=True)
    last = now - days * 86400
    spread = replies(days, {"claude": 90_000, "codex": 120_000, "antigravity": 60_000}[agent]) if number % 3 else replies(days, 30_000)[-40:]
    if agent == "claude":
        ident = f"{number:08d}-0000-4000-8000-000000000000"
        lines = [{"type": "ai-title", "aiTitle": title, "cwd": str(cwd)}]
        for index, (at, tokens) in enumerate(spread):
            lines.append({"type": "assistant", "sessionId": ident, "timestamp": iso(at), "message": {
                "id": f"msg_{number}_{index}", "model": random.choice(models["claude"]), "stop_reason": "end_turn",
                "usage": {"input_tokens": tokens // 40, "cache_read_input_tokens": tokens * 8 // 10,
                          "cache_creation_input_tokens": tokens // 20, "output_tokens": tokens // 25}}})
        path = projects / f"{ident}.jsonl"
        path.write_text("".join(compact(line) + "\n" for line in lines))
    elif agent == "codex":
        ident = f"01a2{number:04d}-0000-7000-8000-{number:012d}"
        born = spread[0][0] if spread else last
        day = dt.datetime.fromtimestamp(born)
        folder_path = codex_dir / "sessions" / day.strftime("%Y/%m/%d")
        folder_path.mkdir(parents=True, exist_ok=True)
        path = folder_path / f"rollout-{day.strftime('%Y-%m-%dT%H-%M-%S')}-{ident}.jsonl"
        lines = [
            {"timestamp": iso(born), "type": "session_meta", "payload": {"id": ident, "cwd": str(cwd), "source": "cli"}},
            {"timestamp": iso(born), "type": "response_item", "payload": {"type": "message", "role": "user", "content": [{"type": "input_text", "text": title}]}},
            {"timestamp": iso(born), "type": "turn_context", "payload": {"model": "gpt-6.2"}},
        ]
        total = 0
        for at, tokens in spread:
            total += tokens
            last_use = {"input_tokens": tokens * 9 // 10, "cached_input_tokens": tokens * 7 // 10, "output_tokens": tokens // 10, "total_tokens": tokens}
            lines.append({"timestamp": iso(at), "type": "event_msg", "payload": {"type": "token_count", "info": {
                "total_token_usage": {"input_tokens": total, "total_tokens": total}, "last_token_usage": last_use},
                "rate_limits": {"primary": {"used_percent": 23.0, "window_minutes": 300, "resets_at": int(now + 7_400)},
                                "secondary": {"used_percent": 41.0, "window_minutes": 10080, "resets_at": int(now + 260_000)}}}})
        path.write_text("".join(compact(line) + "\n" for line in lines))
    else:
        ident = f"6b4c{number:04d}-0000-4000-8000-{number:012d}"
        path = agy_dir / "conversations" / f"{ident}.db"
        path.write_bytes(b"")
        logs = agy_dir / "brain" / ident / ".system_generated" / "logs"
        logs.mkdir(parents=True)
        steps = [{"step_index": index, "type": "PLANNER_RESPONSE", "status": "DONE", "created_at": iso(at),
                  "model": "gemini-4-neon", "input_tokens": tokens * 9 // 10, "output_tokens": tokens // 10}
                 for index, (at, tokens) in enumerate(spread)]
        (logs / "transcript_full.jsonl").write_text("".join(compact(step) + "\n" for step in steps))
        summaries.append((ident, title, len(steps) or 1, dt.datetime.fromtimestamp(last, dt.timezone.utc).strftime("%Y-%m-%d %H:%M:%S+00:00"), json.dumps([f"file://{cwd}"])))
    os.utime(path, (last, last))
    ids[title] = (agent, ident)

with sqlite3.connect(agy_dir / "conversation_summaries.db") as db:
    db.execute("""CREATE TABLE conversation_summaries (conversation_id text PRIMARY KEY, title text NOT NULL DEFAULT '',
        preview text NOT NULL DEFAULT '', step_count integer NOT NULL DEFAULT 0, last_modified_time datetime NOT NULL,
        workspace_uris text NOT NULL, parent_conversation_id text NOT NULL DEFAULT '', nesting_depth integer NOT NULL DEFAULT 0)""")
    db.executemany("INSERT INTO conversation_summaries (conversation_id, preview, step_count, last_modified_time, workspace_uris) VALUES (?, ?, ?, ?, ?)", summaries)

# Plan limits: Claude Code's cached answer to `/usage`, and `agy`'s answer.
(claude_dir / ".claude.json").write_text(json.dumps({"cachedUsageUtilization": {"fetchedAtMs": int(now * 1000), "utilization": {"limits": [
    {"kind": "session", "percent": 34, "resets_at": iso(now + 9_000)},
    {"kind": "weekly_all", "percent": 52, "resets_at": iso(now + 300_000)},
    {"kind": "weekly_scoped", "percent": 18, "resets_at": iso(now + 300_000), "scope": {"model": {"display_name": "Opus"}}}]}}}))
(here / "agy-usage.json").write_text(json.dumps({"status": "SUCCESS", "command": {"data": {"groups": [
    {"name": "Gemini Models", "buckets": [{"window": "5h", "remaining_fraction": 0.82, "reset_time": iso(now + 12_000)},
                                          {"window": "weekly", "remaining_fraction": 0.61, "reset_time": iso(now + 400_000)}]}]}}}))
for name, body in {
    "agent": "#!/bin/sh\nwhile read -r line; do :; done\n",
    "agy-agent": f"#!/bin/sh\n[ \"$1\" = --print ] && exec cat '{here / 'agy-usage.json'}'\nwhile read -r line; do :; done\n",
}.items():
    (here / name).write_text(body)
    (here / name).chmod(0o755)

# A prompt that shows only the folder, and a small project for the terminal.
zdot = home / ".zdotdir"
zdot.mkdir()
(zdot / ".zshrc").write_text("PROMPT='%F{4}%~%f %F{5}❯%f '\nexport CLICOLOR=1\n")
kernel = home / "code" / "kernel"
(kernel / "mm").mkdir(parents=True, exist_ok=True)
(kernel / "mm" / "fault.c").write_text("""// Resolve a page fault: find the area, then fill the page table entry.
#include "mm.h"

int handle_fault(struct mm *mm, unsigned long address, unsigned int flags)
{
	struct area *area = find_area(mm, address);

	if (!area || address < area->start)
		return FAULT_SIGSEGV;
	if ((flags & FAULT_WRITE) && !(area->flags & AREA_WRITE))
		return FAULT_SIGSEGV;

	pte_t *pte = walk_to_pte(mm->pgd, address, true);
	if (!pte)
		return FAULT_OOM;
	if (pte_present(*pte))
		return flags & FAULT_WRITE ? copy_on_write(area, pte, address) : FAULT_NONE;

	struct page *page = area->file ? read_page(area, address) : zero_page(area);
	if (!page)
		return FAULT_OOM;
	set_pte(pte, make_pte(page, area->flags));
	return FAULT_NONE;
}
""")
(kernel / "mm" / "mm.h").write_text("struct mm;\nstruct area;\nint handle_fault(struct mm *mm, unsigned long address, unsigned int flags);\n")
(kernel / "Makefile").write_text("obj-y += mm/fault.o mm/slab.o sched/fair.o\n")
git = lambda *args, when=now: subprocess.run(["git", *args], cwd=kernel, check=True, capture_output=True, env={
    **os.environ, "GIT_AUTHOR_DATE": iso(when), "GIT_COMMITTER_DATE": iso(when), "HOME": str(home)})
git("init", "-q", "-b", "main")
git("config", "user.name", "Deck Demo")
git("config", "user.email", "demo@example.invalid")
for days, message in [(30, "Start a toy kernel"), (24, "Map the kernel image"), (19, "Add a slab allocator"),
                      (15, "Walk page tables to the PTE"), (11, "Fault in anonymous pages"), (8, "Copy on write"),
                      (5, "Read file pages on fault"), (3, "Count faults per area"), (1, "Reject writes to read-only areas")]:
    (kernel / "NOTES").write_text(message + "\n")
    git("add", "-A")
    git("commit", "-q", "-m", message, when=now - days * 86400)

# The store: domains, links and assignments, and a profile.
env = {"HOME": str(home), "PATH": "/usr/bin:/bin", "CLAUDE_CONFIG_DIR": str(claude_dir), "CODEX_HOME": str(codex_dir),
       "NOEMA_DECK_ANTIGRAVITY_HOME": str(agy_dir)}
server = subprocess.Popen([cli, "--store", store, "serve", "--ui", ui, "--port", "0"],
                          env=env, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, text=True)
address = server.stdout.readline().split()[-1]

def ask(command, args=None):
    body = json.dumps({"command": command, **({"args": args} if args else {})}).encode()
    request = urllib.request.Request(f"{address}/map", body, {"Content-Type": "application/json"})
    return json.load(urllib.request.urlopen(request))

try:
    made = {}
    for name, (x, y, parents) in domains.items():
        args = {"name": name, "position": [x, y]} if not parents else {"name": name, "parent": made[parents[0]]}
        shot = ask("add_domain", args)
        made[name] = next(item["id"] for item in shot["domains"] if item["name"] == name)
        ask("move_domain", {"domain": made[name], "x": x, "y": y})
        for extra in parents[1:]:
            ask("link", {"parent": made[extra], "child": made[name]})
    for title, _, _, _, domain in sessions:
        if domain:
            agent, ident = ids[title]
            ask("assign", {"domain": made[domain], "sessions": [{"agent": agent, "id": ident}]})
    ask("set_profile", {"profile": {"name": "Kit Noema", "avatar": None}})
    shot = ask("snapshot")
    print(len(shot["domains"]), "domains,", len(shot["links"]), "links,", len(shot["sessions"]), "sessions,",
          len(shot["missing"]), "missing")
finally:
    server.terminate()
