// Interface for Noema Deck. The core owns every rule; this file sends requests,
// hands each snapshot to the sky and fills the lists around it.

import { Sky } from "./sky.js";

const tauri = window.__TAURI__;
// The narrowest, usual and widest a part beside the map can be dragged.
const WIDTHS = { list: [180, 248, 520], panel: [260, 320, 560] };
const QUIET = 3000; // Milliseconds without output before an open session counts as quiet.
const LINGER = 2000; // A pane whose process ends sooner stays up, so what it printed can be read.
const SETTLE = 5000; // Milliseconds a new pane spends drawing itself before its output counts as news.
const CLICK = 220; // Milliseconds a click on a session waits, in case it is the first of a double click.
const RECENT = 40; // Sessions the list shows before a search narrows it.
const FONT = 'ui-monospace, "SF Mono", Menlo, Consolas, "Symbols Nerd Font Mono", monospace';
const $ = (id) => document.getElementById(id);
// The terminal's colours, drawn from the same night as the rest of Deck: soft
// hues of equal weight on the panel colour, so a busy prompt sits in the scene.
const TERMINAL = {
  background: "#0a0c18",
  foreground: "#e4dfd4",
  cursor: "#e6c48a",
  cursorAccent: "#0a0c18",
  selectionBackground: "rgba(142, 160, 220, 0.3)",
  black: "#1c2036",
  red: "#e0857a",
  green: "#9ccb98",
  yellow: "#e6c48a",
  blue: "#8ea2d8",
  magenta: "#b7a0dc",
  cyan: "#86c2cc",
  white: "#d6d1c6",
  brightBlack: "#4b5272",
  brightRed: "#ee9d92",
  brightGreen: "#b3dcae",
  brightYellow: "#f0d5a3",
  brightBlue: "#a9bbea",
  brightMagenta: "#cbb7ec",
  brightCyan: "#a0d6de",
  brightWhite: "#f2eee5",
};

// Icons are drawn here, sixteen units square, so a button needs no word.
const ICONS = {
  close: "M4 4l8 8M12 4l-8 8",
  hide: "M4 11.5h8",
  right: "M2.5 3.5h11v9h-11zM8 3.5v9",
  down: "M2.5 3.5h11v9h-11zM2.5 8h11",
  again: "M12.4 8.6a4.5 4.5 0 1 1-1.2-3.7M12.6 2.6v2.7H9.9",
  locate: "M8 14s4-3.6 4-7a4 4 0 0 0-8 0c0 3.4 4 7 4 7zM8 8.4a1.4 1.4 0 1 0 0-2.8 1.4 1.4 0 0 0 0 2.8z",
};

function icon(name) {
  const svg = document.createElementNS("http://www.w3.org/2000/svg", "svg");
  svg.setAttribute("viewBox", "0 0 16 16");
  svg.append(document.createElementNS("http://www.w3.org/2000/svg", "path"));
  svg.firstChild.setAttribute("d", ICONS[name]);
  return svg;
}

export const sky = new Sky($("sky"));

export const state = {
  snapshot: { domains: [], links: [], sessions: [], missing: [], panes: { open: [], layout: null } },
  view: "map", // "map" or "shell".
  listed: true, // Whether the session list is showing.
  widths: { list: WIDTHS.list[1], panel: WIDTHS.panel[1] },
  focus: null, // ID of the focused domain.
  nested: false, // Whether the panel includes sessions of subdomains.
  query: "",
  picked: new Set(), // Keys of the sessions ticked in the history picker.
};

let drawn = ""; // The snapshot on screen, as the text it arrived in.

// Requests

/** Sends one request down a route: "map" for domains and sessions, "pane" for a terminal. */
async function send(route, request) {
  if (tauri) {
    try {
      return await tauri.core.invoke(route, { request });
    } catch (error) {
      throw new Error(String(error));
    }
  }
  const response = await fetch(`/${route}`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(request),
  });
  const body = await response.json();
  if (!response.ok) throw new Error(body.error);
  return body;
}

const call = (command, args) => send("map", args ? { command, args } : { command });
const paneCall = (command, args) => send("pane", { command, args });

/** Sends a request and draws the answer. Resolves to false when it is refused. */
async function run(command, args) {
  let snapshot;
  try {
    snapshot = await call(command, args);
  } catch (error) {
    toast(error.message);
    return false;
  }
  // Most refreshes find nothing new, and then nothing is drawn again.
  const text = JSON.stringify(snapshot);
  if (text !== drawn) {
    [drawn, state.snapshot] = [text, snapshot];
    render();
  }
  return true;
}

// Helpers

const keyOf = (session) => `${session.agent}/${session.id}`;
const refOf = (session) => ({ agent: session.agent, id: session.id });
const domainById = (id) => state.snapshot.domains.find((domain) => domain.id === id);
const sessionByKey = (key) => state.snapshot.sessions.find((session) => keyOf(session) === key);
const matches = (session, query) =>
  `${session.title}\n${session.folder ?? ""}`.toLowerCase().includes(query.trim().toLowerCase());

function ago(seconds) {
  const elapsed = Date.now() / 1000 - seconds;
  if (elapsed < 3600) return `${Math.max(1, Math.round(elapsed / 60))}m`;
  if (elapsed < 86400) return `${Math.round(elapsed / 3600)}h`;
  if (elapsed < 30 * 86400) return `${Math.round(elapsed / 86400)}d`;
  return new Date(seconds * 1000).toLocaleDateString();
}

function el(tag, props = {}, ...children) {
  const node = Object.assign(document.createElement(tag), props);
  node.append(...children.filter(Boolean));
  return node;
}

/** A button showing `content`, a word or an icon, that says `label` when asked. */
function button(className, content, label, act) {
  const node = el("button", { className, type: "button", title: label }, content);
  node.setAttribute("aria-label", label);
  node.addEventListener("click", (event) => {
    event.stopPropagation();
    act();
  });
  return node;
}

/** Shows a message: a refusal, or with `action` a calm note offering one step, such as undoing. */
function toast(message, action) {
  const [text, offer] = $("toast").children;
  text.textContent = message;
  offer.hidden = !action;
  $("toast").classList.toggle("calm", Boolean(action));
  if (action) {
    offer.textContent = action.label;
    offer.onclick = () => {
      $("toast").hidden = true;
      action.act();
    };
  }
  $("toast").hidden = false;
  clearTimeout(toast.timer);
  toast.timer = setTimeout(() => ($("toast").hidden = true), action ? 8000 : 5000);
}


// Views: the map or the terminals fill the window, with the session list beside either

const freeLeft = () => (state.listed ? state.widths.list : 0);
const freeRight = () => innerWidth - ($("panel").hidden ? 0 : state.widths.panel);
const fit = () => sky.fit(freeLeft(), freeRight());
/** ID of the domain drawn under a screen point, unless something covers the map there. */
const domainUnder = (x, y) => (document.elementFromPoint(x, y) === $("sky") ? sky.starAt(x, y) : null);

export function show(view) {
  state.view = view;
  const shell = view === "shell";
  $("shell").hidden = !shell;
  $("show-map").classList.toggle("on", !shell);
  $("show-shell").classList.toggle("on", shell);
  document.body.classList.toggle("on-map", !shell);
  renderPanel();
  where();
  if (!shell) document.activeElement?.blur();
  arrange();
}

/** Keeps a choice about the layout for the next run. The layout still works when it cannot be kept. */
function keep(name, value) {
  try {
    localStorage.setItem(`noema-deck-${name}`, value);
  } catch {
    // Nothing to do.
  }
}

function kept(name) {
  try {
    return localStorage.getItem(`noema-deck-${name}`);
  } catch {
    return null;
  }
}

export function showList(listed) {
  state.listed = listed;
  document.body.classList.toggle("listed", listed);
  $("toggle-list").classList.toggle("on", listed);
  keep("list", listed ? "shown" : "hidden");
}

/** Sets the width of the session list or the domain panel, or puts it back to the usual one. */
export function setWidth(part, width) {
  const [least, usual, most] = WIDTHS[part];
  state.widths[part] = Math.round(Math.min(most, Math.max(least, width || usual)));
  document.body.style.setProperty(`--${part}`, `${state.widths[part]}px`);
  keep(`${part}-width`, state.widths[part]);
}

/** Follows the pointer from a press on `handle` until it lets go, calling `move` with each event. */
function dragging(handle, move) {
  handle.addEventListener("pointerdown", (event) => {
    if (event.button !== 0) return;
    event.preventDefault();
    handle.setPointerCapture(event.pointerId);
    handle.classList.add("held");
    const stop = () => {
      handle.classList.remove("held");
      handle.removeEventListener("pointermove", move);
    };
    handle.addEventListener("pointermove", move);
    handle.addEventListener("pointerup", stop, { once: true });
    handle.addEventListener("pointercancel", stop, { once: true });
  });
}

// Each edge drags to resize, and a double click puts it back.
dragging($("list-edge"), (event) => setWidth("list", event.clientX));
dragging($("panel-edge"), (event) => setWidth("panel", innerWidth - event.clientX));
$("list-edge").addEventListener("dblclick", () => setWidth("list"));
$("panel-edge").addEventListener("dblclick", () => setWidth("panel"));

/** Names what fills the window in the middle of the top bar: the focused domain, or the active pane. */
function where() {
  const text = state.view === "shell" ? (active ? paneName(active) : "") : (domainById(state.focus)?.name ?? "");
  $("where").textContent = text;
}

function focus(id) {
  Object.assign(state, { focus: id, nested: false });
  sky.mark({ focus: id });
  renderPanel();
  where();
  const domain = domainById(id);
  if (domain) sky.bring(domain.x, domain.y, (freeLeft() + freeRight()) / 2, innerHeight / 2);
}

// Drawing

/** Draws a new snapshot. */
function render() {
  const { domains, links } = state.snapshot;
  if (state.focus !== null && !domainById(state.focus)) state.focus = null;
  sky.chart(domains, links);
  sky.mark({ focus: state.focus });
  renderPanel();
  renderList();
  if (!$("picker").hidden) renderPicker();
  $("empty").hidden = domains.length > 0;
  for (const pane of panes) {
    if (!pane.title.querySelector("input")) pane.title.textContent = paneName(pane);
  }
  where();
}

/** What a pane is called: its session's title, or the name given to its shell. */
function paneName(pane) {
  return sessionByKey(pane.key)?.title ?? pane.name ?? (pane.shell ? "Shell" : "New session");
}

/** The name a pane or a session goes by, and how a new one is given to it. */
function renamer(pane, session) {
  session ??= pane && sessionByKey(pane.key);
  if (session) return (title) => run("rename_session", { session: refOf(session), title });
  if (!pane?.shell) return null;
  return (title) => {
    pane.name = title;
    pane.title.textContent = title;
    renderList();
    where();
    remember();
  };
}

/** Acts on a click after a moment, unless a second click makes it a double click. */
function clicks(node, once, twice) {
  let timer = 0;
  node.addEventListener("click", (event) => {
    if (event.target.closest("button, input") || Date.now() - carried < 300) return;
    clearTimeout(timer);
    if (event.detail < 2) timer = setTimeout(once, CLICK);
  });
  node.addEventListener("dblclick", (event) => {
    if (event.target.closest("button, input")) return;
    clearTimeout(timer);
    twice();
  });
}

/** Turns a title into a box for a new name, given to `apply`. Enter or leaving the box keeps it, Escape puts the old one back. */
function rename(node, apply) {
  if (!apply || node.querySelector("input")) return;
  const old = node.textContent;
  const input = el("input", { className: "rename", value: old, spellcheck: false });
  input.setAttribute("aria-label", "New name");
  node.replaceChildren(input);
  input.focus();
  input.select();
  let done = false;
  const finish = (keep) => {
    if (done) return;
    done = true;
    const title = input.value.trim();
    node.textContent = old;
    if (keep && title && title !== old) apply(title);
  };
  input.addEventListener("keydown", (event) => {
    event.stopPropagation();
    if (event.key === "Enter") finish(true);
    else if (event.key === "Escape") finish(false);
  });
  input.addEventListener("blur", () => finish(true));
  // The row or header around the box opens, drags or zooms on these.
  for (const type of ["click", "dblclick", "pointerdown"]) input.addEventListener(type, (event) => event.stopPropagation());
}

/** The chips naming the domains a session is assigned to, leaving one out. */
function chips(session, except) {
  const names = session.domains.filter((id) => id !== except).map((id) => domainById(id)?.name);
  if (!names.length) return null;
  return el("div", { className: "chips" }, ...names.map((name) => el("span", { className: "chip plain", textContent: name })));
}

// The session list

function listRow(title, sub, pane, session) {
  const dot = el("span", { className: "dot" });
  const apply = renamer(pane, sessionByKey(keyOf(session)));
  const text = el("span", { className: "title", textContent: title, title: apply ? `${title}\nDouble-click to rename` : title });
  const tools = el("span", { className: "tools" });
  const row = el("li", { className: "entry", tabIndex: 0, title: "Drag into the terminal to open it there" }, dot, text, el("span", { className: "sub", textContent: sub }), tools);
  const open = () => (pane ? showPane(pane) : openSession(session));
  if (pane) {
    pane.dot = dot;
    tools.append(button("close", icon("close"), "Stop and close", () => closePane(pane)));
    row.classList.toggle("current", state.view === "shell" && shown.includes(pane));
  }
  clicks(row, open, () => rename(text, apply));
  carriable(row, () => (pane ? { pane, session: sessionByKey(pane.key) } : { session }));
  row.addEventListener("keydown", (event) => event.target === row && event.key === "Enter" && open());
  return row;
}

function renderList() {
  const { sessions } = state.snapshot;
  const describe = (session) => {
    const domain = domainById(session.domains[0])?.name;
    return [domain, ago(session.updated)].filter(Boolean).join(", ");
  };
  const open = panes.flatMap((pane) => {
    const session = sessionByKey(pane.key) ?? { title: paneName(pane), folder: pane.folder, domains: [] };
    if (!matches(session, state.query)) return [];
    return [listRow(session.title, session.updated ? describe(session) : (pane.folder ?? ""), pane, session)];
  });
  const running = new Set(panes.map((pane) => pane.key));
  let recent = sessions.filter(
    (session) => session.domains.length && !running.has(keyOf(session)) && matches(session, state.query),
  );
  if (!state.query.trim()) recent = recent.slice(0, RECENT);

  $("open-list").replaceChildren(...open);
  $("recent-list").replaceChildren(...recent.map((session) => listRow(session.title, describe(session), null, session)));
  $("open-head").hidden = !open.length;
  $("recent-head").hidden = !recent.length;
  let note = "";
  if (!open.length && !recent.length) {
    note = state.query.trim() ? "No session matches." : "Sessions assigned to a domain appear here.";
  }
  $("list-note").textContent = note;
  tick();
}

/** Whether a pane is where the person can see it. */
const seen = (pane) => state.view === "shell" && shown.includes(pane) && document.hasFocus();

/** Marks each open session as active, quiet, ready or ended. Ready is quiet after output nobody saw. */
function tick() {
  const now = Date.now();
  let ready = false;
  for (const pane of panes) {
    if (seen(pane)) pane.unseen = false;
    const quiet = now - pane.heard >= QUIET;
    const mood = pane.ended ? "ended" : !quiet ? "active" : pane.unseen ? "ready" : "quiet";
    ready ||= mood === "ready";
    if (pane.dot) pane.dot.className = `dot ${mood}`;
  }
  $("show-shell").classList.toggle("ready", ready);
}

setInterval(tick, 1000);

$("list-search").addEventListener("input", () => {
  state.query = $("list-search").value;
  renderList();
});

$("toggle-list").addEventListener("click", () => showList(!state.listed));
$("new-session").addEventListener("click", () => openStarter($("new-session")));
$("new-shell").addEventListener("click", () => openShell());

// The domain panel

function sessionRow(session, domain) {
  const title = el("div", { className: "title", textContent: session.title, title: `${session.title}\nDouble-click to rename` });
  const tools = el("div", { className: "tools" }, button("open", "Open", "Open in a terminal", () => openSession(session)));
  if (session.domains.includes(domain)) tools.append(removeButton(domain, session));
  const folder = el("div", { className: "folder", textContent: session.folder ?? "No folder", title: session.folder ?? "" });
  const when = el("div", { className: "when", textContent: ago(session.updated) });
  const row = el("li", { className: "session" }, title, tools, folder, when, chips(session, domain));
  row.addEventListener("dblclick", (event) => !event.target.closest("button, input") && rename(title, renamer(null, session)));
  carriable(row, () => ({ session, assignOnly: true }));
  return row;
}

function removeButton(domain, session) {
  return button("remove", "×", "Remove from this domain", () => run("unassign", { domain, session: refOf(session) }));
}

function linkRow(label, domains, unlink) {
  const chips = domains.map((domain) => {
    const go = button("go", domain.name, `Go to ${domain.name}`, () => focus(domain.id));
    const drop = button("drop", "×", `Unlink ${domain.name}`, () => unlink(domain.id));
    return el("span", { className: "chip" }, go, drop);
  });
  return el("div", { className: "link-row" }, label, ...chips);
}

function renderPanel() {
  const domain = domainById(state.focus);
  $("panel").hidden = !domain || state.view !== "map";
  document.body.classList.toggle("focused", !$("panel").hidden);
  if (!domain) return;
  const { links, sessions, missing } = state.snapshot;
  if (document.activeElement !== $("domain-name")) $("domain-name").value = domain.name;
  $("delete-domain").textContent = "Delete";
  $("nested").checked = state.nested;

  const parents = links.filter(([, child]) => child === domain.id).map(([id]) => domainById(id));
  const children = links.filter(([id]) => id === domain.id).map(([, id]) => domainById(id));
  const rows = [];
  if (parents.length) {
    rows.push(linkRow("Parents", parents, (id) => run("unlink", { parent: id, child: domain.id })));
  }
  if (children.length) {
    rows.push(linkRow("Subdomains", children, (id) => run("unlink", { parent: domain.id, child: id })));
  }
  $("domain-links").replaceChildren(...rows);

  const all = new Set(domain.sessions.map(keyOf));
  const here = (session) => session.domains.includes(domain.id);
  const shown = state.nested ? (session) => all.has(keyOf(session)) : here;
  const count = sessions.filter(here).length + missing.filter(here).length;
  $("domain-meta").textContent =
    domain.sessions.length === count
      ? `${count} ${count === 1 ? "session" : "sessions"}`
      : `${count} here, ${domain.sessions.length} with subdomains`;
  const gone = missing.filter(shown).map((session) => {
    const text = `Missing session ${session.id.slice(0, 8)}`;
    const tools = here(session) && el("div", { className: "tools" }, removeButton(domain.id, session));
    return el("li", { className: "session missing" }, el("div", { className: "title", textContent: text }), tools);
  });
  $("domain-sessions").replaceChildren(
    ...sessions.filter(shown).map((session) => sessionRow(session, domain.id)),
    ...gone,
  );
}

$("domain-name").addEventListener("change", async () => {
  const renamed = await run("rename_domain", { domain: state.focus, name: $("domain-name").value });
  if (!renamed) $("domain-name").value = domainById(state.focus)?.name ?? "";
});

$("domain-name").addEventListener("keydown", (event) => {
  if (event.key === "Enter") $("domain-name").blur();
});

$("close-panel").addEventListener("click", () => focus(null));
$("start-here").addEventListener("click", () => openStarter($("start-here"), state.focus));
$("add-existing").addEventListener("click", openPicker);

$("add-subdomain").addEventListener("click", () => {
  const box = $("add-subdomain").getBoundingClientRect();
  const parent = state.focus;
  openNamer(box.left, box.top - 44, (text) => addDomain(text, { parent }));
});

$("delete-domain").addEventListener("click", () => {
  if ($("delete-domain").textContent === "Delete") $("delete-domain").textContent = "Confirm delete";
  else run("delete_domain", { domain: state.focus });
});

$("nested").addEventListener("change", () => {
  state.nested = $("nested").checked;
  renderPanel();
});

// Starting a session: choose a domain and a folder

function openStarter(anchor, domain) {
  if (!$("starter").hidden && openStarter.anchor === anchor) return closeStarter();
  const { domains, sessions, last_folder: last } = state.snapshot;
  if (!domains.length) return toast("Add a domain first: double-click the map.");
  openStarter.anchor = anchor;
  const byName = [...domains].sort((a, b) => a.name.localeCompare(b.name));
  $("starter-domain").replaceChildren(...byName.map(({ id, name }) => el("option", { value: id, textContent: name })));
  const current = sessionByKey(active?.key)?.domains[0];
  $("starter-domain").value = domain ?? state.focus ?? current ?? byName[0].id;
  const folders = [...new Set(sessions.map((session) => session.folder).filter(Boolean))];
  $("folders").replaceChildren(...folders.map((value) => el("option", { value })));
  $("starter-folder").value = last ?? folders[0] ?? "~";
  $("starter-name").value = "";

  const box = anchor.getBoundingClientRect();
  $("starter").style.left = `${Math.max(8, Math.min(box.left, innerWidth - 308))}px`;
  $("starter").style.top = `${Math.min(box.bottom + 6, innerHeight - 124)}px`;
  $("starter").hidden = false;
  $("starter-name").focus();
}

function closeStarter() {
  $("starter").hidden = true;
}

$("starter").addEventListener("submit", (event) => {
  event.preventDefault();
  const folder = $("starter-folder").value.trim();
  if (!folder) return toast("Enter a folder for the session.");
  closeStarter();
  startSession(Number($("starter-domain").value), folder, $("starter-name").value.trim() || null);
});

document.addEventListener("pointerdown", (event) => {
  if (!event.target.closest("#starter") && event.target !== openStarter.anchor) closeStarter();
  if (!event.target.closest("#help, #show-help")) $("help").hidden = true;
});

$("show-help").addEventListener("click", () => ($("help").hidden = !$("help").hidden));

// Adding sessions from the agent's history to the focused domain

function openPicker() {
  state.picked.clear();
  $("picker-search").value = "";
  $("picker").hidden = false;
  renderPicker();
  $("picker-search").focus();
}

function closePicker() {
  $("picker").hidden = true;
}

function renderPicker() {
  const domain = domainById(state.focus);
  if (!domain) return closePicker();
  $("picker-title").textContent = `Add sessions to ${domain.name}`;
  const rows = state.snapshot.sessions
    .filter((session) => !session.domains.includes(domain.id) && matches(session, $("picker-search").value))
    .map((session) => {
      const key = keyOf(session);
      const title = el("div", { className: "title", textContent: session.title, title: session.title });
      const when = el("div", { className: "when", textContent: ago(session.updated) });
      const folder = el("div", { className: "folder", textContent: session.folder ?? "No folder" });
      const row = el("li", { className: "session pick" }, title, when, folder, chips(session));
      row.classList.toggle("selected", state.picked.has(key));
      row.addEventListener("click", () => {
        if (!state.picked.delete(key)) state.picked.add(key);
        row.classList.toggle("selected", state.picked.has(key));
        countPicked();
      });
      return row;
    });
  $("picker-list").replaceChildren(...rows);
  $("picker-note").textContent = rows.length ? "" : "No session matches.";
  countPicked();
}

function countPicked() {
  $("picker-add").textContent = state.picked.size ? `Add ${state.picked.size}` : "Add";
  $("picker-add").disabled = !state.picked.size;
}

$("picker-search").addEventListener("input", renderPicker);
$("picker-cancel").addEventListener("click", closePicker);
$("picker").addEventListener("pointerdown", (event) => event.target === $("picker") && closePicker());

$("picker").addEventListener("submit", async (event) => {
  event.preventDefault();
  const sessions = [...state.picked].map((key) => refOf(sessionByKey(key)));
  if (await run("assign", { domain: state.focus, sessions })) closePicker();
});

// Naming a new domain

/** Adds one domain, or a folder's tree of domains when the text is a folder's path. */
async function addDomain(text, where) {
  if (!text.startsWith("/") && !text.startsWith("~")) return run("add_domain", { name: text, ...where });
  if (await run("import", { folder: text, ...where })) fit();
}

let whenNamed = null; // What the name box does with the name it is given.

function openNamer(x, y, onName) {
  whenNamed = onName;
  $("namer").style.left = `${Math.min(x, innerWidth - 312)}px`;
  $("namer").style.top = `${Math.min(y, innerHeight - 48)}px`;
  $("namer").hidden = false;
  $("namer-input").value = "";
  $("namer-input").focus();
}

function closeNamer() {
  $("namer").hidden = true;
  whenNamed = null;
}

$("namer").addEventListener("submit", (event) => {
  event.preventDefault();
  const [name, onName] = [$("namer-input").value.trim(), whenNamed];
  if (!name) return toast("Enter a name for the domain.");
  closeNamer();
  onName(name);
});
$("namer-input").addEventListener("blur", closeNamer);

// The map: pan, zoom, move, focus and link

let drag = null;

$("sky").addEventListener("pointerdown", (event) => {
  if (event.button !== 0) return;
  sky.hold();
  const [x, y] = [event.clientX, event.clientY];
  const domain = domainById(sky.starAt(x, y));
  const kind = !domain ? "pan" : event.shiftKey ? "link" : "move";
  drag = { kind, domain, origin: domain && { x: domain.x, y: domain.y }, from: [x, y], x, y, moved: false };
  $("sky").setPointerCapture(event.pointerId);
});

$("sky").addEventListener("pointermove", (event) => {
  const [x, y] = [event.clientX, event.clientY];
  if (!drag) {
    const hover = sky.starAt(x, y);
    $("sky").classList.toggle("over", hover !== null);
    return sky.mark({ hover });
  }
  const [dx, dy] = [x - drag.from[0], y - drag.from[1]];
  if (!drag.moved && Math.hypot(dx, dy) < 4) return;
  drag.moved = true;
  $("sky").classList.add("dragging");
  if (drag.kind === "pan") {
    sky.panBy(x - drag.x, y - drag.y);
  } else if (drag.kind === "move") {
    drag.domain.x = drag.origin.x + dx / sky.view.k;
    drag.domain.y = drag.origin.y + dy / sky.view.k;
    sky.redraw();
  } else {
    sky.mark({ rubber: { from: drag.domain.id, x, y }, drop: domainUnder(x, y) });
  }
  [drag.x, drag.y] = [x, y];
});

$("sky").addEventListener("pointerup", async (event) => {
  const done = drag;
  drag = null;
  if (!done) return;
  $("sky").classList.remove("dragging");
  sky.mark({ rubber: null, drop: null });
  if (!done.moved) return focus(done.domain?.id ?? null);
  if (done.kind === "pan") {
    sky.release();
  } else if (done.kind === "move") {
    const { id: domain, x, y } = done.domain;
    if (await run("move_domain", { domain, x, y })) return;
    Object.assign(done.domain, done.origin);
    sky.redraw();
  } else {
    const child = domainUnder(event.clientX, event.clientY);
    if (child !== null && child !== done.domain.id) run("link", { parent: done.domain.id, child });
  }
});

$("sky").addEventListener("pointerleave", () => sky.mark({ hover: null }));

$("sky").addEventListener("dblclick", (event) => {
  if (sky.starAt(event.clientX, event.clientY) !== null) return;
  const position = sky.toWorld(event.clientX, event.clientY);
  openNamer(event.clientX, event.clientY, (text) => addDomain(text, { position }));
});

$("sky").addEventListener(
  "wheel",
  (event) => {
    event.preventDefault();
    // A pinch arrives as a wheel with the control key down, in much finer steps.
    const rate = event.ctrlKey ? 0.012 : 0.0018;
    sky.zoomAt(event.clientX, event.clientY, Math.exp(-event.deltaY * rate));
  },
  { passive: false },
);

// Arranging the whole map, for a view of every domain at once

/** Places every domain on rings, and offers to put them back. */
async function arrangeMap() {
  const before = state.snapshot.domains.map(({ id, x, y }) => [id, x, y]);
  if (!before.length || !(await run("arrange"))) return;
  glide(before);
  toast("Arranged every domain.", {
    label: "Undo",
    act: async () => {
      const after = state.snapshot.domains.map(({ id, x, y }) => [id, x, y]);
      if (await run("move_domains", { moves: before })) glide(after);
    },
  });
}

/** Shows every domain in its new place, and lets each drift there from where it was. */
function glide(from) {
  fit();
  if (sky.still) return;
  const domains = state.snapshot.domains;
  const start = new Map(from.map(([id, x, y]) => [id, [x, y]]));
  const end = new Map(domains.map(({ id, x, y }) => [id, [x, y]]));
  const began = performance.now();
  const step = (now) => {
    // A newer snapshot has taken over the drawing.
    if (state.snapshot.domains !== domains) return;
    const t = Math.min(1, (now - began) / 550);
    const eased = 1 - (1 - t) ** 3;
    for (const domain of domains) {
      const [[x0, y0], [x1, y1]] = [start.get(domain.id) ?? end.get(domain.id), end.get(domain.id)];
      [domain.x, domain.y] = [x0 + (x1 - x0) * eased, y0 + (y1 - y0) * eased];
    }
    sky.redraw();
    if (t < 1) requestAnimationFrame(step);
  };
  requestAnimationFrame(step);
}

$("arrange-map").addEventListener("click", arrangeMap);
$("fit-map").addEventListener("click", () => fit());

// Carrying a session or a pane: onto a domain to assign it, or into the
// terminal, where the spot it is dropped on says where it opens

let carry = null;
let carried = 0; // When the last carry ended, so the click that ends it opens nothing.
const EDGE = 0.28; // The share of a pane, from each edge, where a drop splits it.

/** Lets `node` be carried. `what` says what it carries: `{ session }`, `{ pane }`, and `assignOnly` for a domain's own list. */
function carriable(node, what) {
  node.addEventListener("pointerdown", (event) => {
    if (event.button !== 0 || event.target.closest("button, input")) return;
    carry = { what: what(), x: event.clientX, y: event.clientY, moved: false, drop: null };
  });
}

const inside = (box, x, y) => x >= box.left && x < box.right && y >= box.top && y < box.bottom;

/** Where a carry would land at a screen point, or null where it cannot. */
function dropAt(x, y) {
  const { pane, session, assignOnly } = carry.what;
  if (state.view === "map") {
    const domain = session ? domainUnder(x, y) : null;
    if (domain !== null) return { domain };
    if (assignOnly || document.elementFromPoint(x, y) !== $("sky")) return null;
    const top = $("top").getBoundingClientRect().bottom;
    return { rect: { left: freeLeft(), top, right: freeRight(), bottom: innerHeight } };
  }
  const box = $("shell").getBoundingClientRect();
  if (assignOnly || !inside(box, x, y)) return null;
  const target = shown.find((item) => inside(item.node.getBoundingClientRect(), x, y));
  if (!target) return { rect: box };
  if (target === pane) return null;
  const rect = target.node.getBoundingClientRect();
  const [across, down] = [(x - rect.left) / rect.width, (y - rect.top) / rect.height];
  const nearest = Object.entries({ left: across, right: 1 - across, top: down, bottom: 1 - down }).sort((p, q) => p[1] - q[1])[0];
  if (nearest[1] > EDGE) return { target, how: "replace", rect };
  const side = nearest[0];
  const how = side === "left" || side === "right" ? "row" : "column";
  const before = side === "left" || side === "top";
  const half = {
    left: { ...rect.toJSON(), right: rect.left + rect.width / 2 },
    right: { ...rect.toJSON(), left: rect.left + rect.width / 2 },
    top: { ...rect.toJSON(), bottom: rect.top + rect.height / 2 },
    bottom: { ...rect.toJSON(), top: rect.top + rect.height / 2 },
  }[side];
  return { target, how, before, rect: half };
}

/** Outlines where a carry would land. */
function outline(drop) {
  const box = $("drop");
  const appearing = box.hidden;
  box.hidden = !drop?.rect;
  sky.mark({ drop: drop?.domain ?? null });
  if (!drop?.rect) return;
  // It glides between spots, but appears where it is.
  box.style.transition = appearing ? "none" : "";
  const { left, top, right, bottom } = drop.rect;
  Object.assign(box.style, { left: `${left + 4}px`, top: `${top + 4}px`, width: `${right - left - 8}px`, height: `${bottom - top - 8}px` });
}

document.addEventListener("pointermove", (event) => {
  if (!carry) return;
  if (!carry.moved) {
    if (Math.hypot(event.clientX - carry.x, event.clientY - carry.y) < 5) return;
    carry.moved = true;
    const { pane, session } = carry.what;
    $("ghost").textContent = pane ? paneName(pane) : session.title;
    $("ghost").hidden = false;
    document.body.classList.add("carrying");
  }
  $("ghost").style.left = `${event.clientX + 12}px`;
  $("ghost").style.top = `${event.clientY + 12}px`;
  carry.drop = dropAt(event.clientX, event.clientY);
  outline(carry.drop);
});

document.addEventListener("pointerup", () => {
  const done = carry;
  carry = null;
  if (!done?.moved) return;
  carried = Date.now();
  $("ghost").hidden = true;
  document.body.classList.remove("carrying");
  outline(null);
  const { drop, what } = done;
  if (!drop) return;
  if (drop.domain !== undefined) return run("assign", { domain: drop.domain, sessions: [refOf(what.session)] });
  const spot = drop.target ? { how: drop.how, before: drop.before, target: drop.target } : "replace";
  const pane = what.pane ?? panes.find((item) => item.key === keyOf(what.session));
  if (pane) showPane(pane, spot);
  else openSession(what.session, spot);
});

// Terminals: each runs the agent's own interface for one session, or a shell

export const panes = []; // Every open pane, in the order opened.
const shown = []; // The panes on screen, in reading order. Kept to match the layout.
let active = null; // The pane that last had the keyboard.
// How the panes on screen tile: `{ pane }`, or `{ dir, items, sizes }` with
// `dir` "row" for side by side or "column" for one above another.
let layout = null;
let built = ""; // The layout now on the page, as text, so an unchanged one is not built again.

const leaves = (node) => (!node ? [] : node.pane ? [node.pane] : node.items.flatMap(leaves));

/** The leaf holding `pane`, with the split it sits in and its place there. */
function leafOf(pane, node = layout, parent = null, index = 0) {
  if (!node) return null;
  if (node.pane) return node.pane === pane ? { leaf: node, parent, index } : null;
  for (const [at, item] of node.items.entries()) {
    const found = leafOf(pane, item, node, at);
    if (found) return found;
  }
  return null;
}

/**
 * Puts `pane` on screen: in place of `target`, by default the active pane, or
 * beside it ("row") or below it ("column"), or `before` it, to the left or above.
 */
function place(pane, how = "replace", before = false, target = active) {
  if (leafOf(pane)) return;
  const spot = leafOf(target) ?? (layout && leafOf(leaves(layout)[0]));
  if (!spot) {
    layout = { pane };
    return;
  }
  const { leaf, parent, index } = spot;
  if (how === "replace") {
    leaf.pane = pane;
  } else if (parent?.dir === how) {
    const half = parent.sizes[index] / 2;
    parent.items.splice(before ? index : index + 1, 0, { pane });
    parent.sizes.splice(index, 1, half, half);
  } else {
    // The leaf becomes a split of its own pane and the new one.
    const old = { pane: leaf.pane };
    delete leaf.pane;
    Object.assign(leaf, { dir: how, items: before ? [{ pane }, old] : [old, { pane }], sizes: [1, 1] });
  }
}

/** Takes `pane` off screen. A split left with one part becomes that part. */
function unplace(pane) {
  const found = leafOf(pane);
  if (!found) return;
  const { parent, index } = found;
  if (!parent) {
    layout = null;
    return;
  }
  parent.items.splice(index, 1);
  parent.sizes.splice(index, 1);
  if (parent.items.length === 1) {
    const [only] = parent.items;
    for (const name of Object.keys(parent)) delete parent[name];
    Object.assign(parent, only);
  }
}

/** Builds the page for a part of the layout. */
function build(node) {
  if (node.pane) return node.pane.node;
  const box = el("div", { className: `split ${node.dir}` });
  node.items.forEach((item, at) => {
    if (at) box.append(divider(node, at - 1));
    const part = build(item);
    part.style.flex = `${node.sizes[at]} 1 0`;
    box.append(part);
  });
  return box;
}

/** The edge between two parts of a split, which drags to share the room between them. */
function divider(node, at) {
  const handle = el("div", { className: "divider", title: "Drag to resize" });
  handle.setAttribute("role", "separator");
  const share = (fraction) => {
    const total = node.sizes[at] + node.sizes[at + 1];
    node.sizes[at] = total * fraction;
    node.sizes[at + 1] = total * (1 - fraction);
    handle.previousElementSibling.style.flex = `${node.sizes[at]} 1 0`;
    handle.nextElementSibling.style.flex = `${node.sizes[at + 1]} 1 0`;
    rememberSoon();
  };
  dragging(handle, (event) => {
    const [before, after] = [handle.previousElementSibling, handle.nextElementSibling].map((part) => part.getBoundingClientRect());
    const row = node.dir === "row";
    const [start, end] = row ? [before.left, after.right] : [before.top, after.bottom];
    const fraction = ((row ? event.clientX : event.clientY) - start) / (end - start);
    share(Math.min(0.9, Math.max(0.1, fraction)));
  });
  handle.addEventListener("dblclick", () => share(0.5));
  return handle;
}

/** The layout as text naming each pane, which says whether the page must be built again. */
const shape = (node) => (!node ? "" : node.pane ? `${panes.indexOf(node.pane)}` : `${node.dir}(${node.items.map(shape).join(",")})`);

/** Puts the terminals on screen as the layout tiles them, and keeps the rest running out of sight. */
function arrange() {
  shown.splice(0, shown.length, ...leaves(layout));
  const now = shape(layout);
  if (now !== built) {
    built = now;
    // Every pane goes back to the terminal before the splits around them are built again.
    for (const pane of panes) $("shell").append(pane.node);
    for (const box of $("shell").querySelectorAll(":scope > .split")) box.remove();
    if (layout) {
      const root = build(layout);
      root.style.flex = "1 1 0";
      $("shell").append(root);
    }
  }
  for (const pane of panes) {
    pane.node.hidden = !shown.includes(pane);
    pane.node.classList.toggle("active", pane === active && shown.length > 1);
    pane.hide.hidden = shown.length < 2;
    pane.again.hidden = !pane.ended;
    pane.locate.hidden = !sessionByKey(pane.key)?.domains.length;
  }
  $("shell-empty").hidden = shown.length > 0;
  if (state.view === "shell") {
    for (const pane of shown) pane.fit.fit();
    active?.term.focus();
  }
  where();
  renderList();
  remember();
}

/**
 * Copies a layout, turning each leaf into what `leaf` gives for it, or
 * leaving it out when that is null. A split left with one part becomes that
 * part. `leaf` answers undefined for a split, which is then copied part by part.
 */
function retile(node, leaf) {
  if (node === null || node === undefined) return null;
  const own = leaf(node);
  if (own !== undefined) return own;
  const parts = node.items.map((item, at) => [retile(item, leaf), node.sizes[at]]).filter(([item]) => item !== null);
  if (parts.length < 2) return parts[0]?.[0] ?? null;
  return { dir: node.dir, items: parts.map(([item]) => item), sizes: parts.map(([, size]) => size) };
}

let reopened = false; // Until the last run's panes are open again, the panes now open say nothing about them.
let remembered = ""; // The panes last kept for the next run, as the text sent.

/** Keeps the open panes and their layout in the store, so the next run opens them again. */
function remember() {
  if (!reopened) return;
  const kept = panes.filter((pane) => sessionByKey(pane.key) || pane.shell);
  const open = kept.map((pane) => {
    const session = sessionByKey(pane.key);
    return session ? { kind: "session", session: refOf(session) } : { kind: "shell", folder: pane.folder, name: pane.name ?? null };
  });
  const save = (pane) => (kept.includes(pane) ? kept.indexOf(pane) : null);
  const panesNow = { open, layout: retile(layout, (node) => node.pane && save(node.pane)) };
  const text = JSON.stringify(panesNow);
  if (text === remembered) return;
  remembered = text;
  run("keep_panes", { panes: panesNow });
}

/** Remembers once a drag has settled, and not on every step of it. */
function rememberSoon() {
  clearTimeout(rememberSoon.timer);
  rememberSoon.timer = setTimeout(remember, 300);
}

/** Opens the panes that were open when Deck last stopped, tiled as they were, behind the map. */
async function reopen() {
  const { open, layout: saved } = state.snapshot.panes;
  remembered = JSON.stringify(state.snapshot.panes);
  const opened = open.map((item) => {
    const session = item.kind === "session" ? sessionByKey(keyOf(item.session)) : null;
    if (item.kind === "session" && !session) return null;
    const pane = addPane(session ? session.title : (item.name ?? "Shell"), session ? session.folder : item.folder, session ? keyOf(session) : null);
    Object.assign(pane, { shell: !session, name: session ? undefined : (item.name ?? undefined) });
    return { pane, command: session ? "open" : "shell", args: session ? { session: refOf(session) } : { folder: item.folder } };
  });
  // Turns the saved layout back into panes, leaving out any that could not open.
  const restore = (node) => (typeof node === "number" && opened[node] ? { pane: opened[node].pane } : null);
  layout = retile(saved, (node) => (typeof node === "number" ? restore(node) : undefined));
  active = leaves(layout)[0] ?? null;
  reopened = true;
  arrange();
  for (const { pane, command, args } of opened.filter(Boolean)) {
    launch(pane, command, args).then((reply) => reply && follow(pane));
  }
}

/** The text on screen in one of the shown panes, for the self-test. */
export function screenText(at = 0) {
  const buffer = shown[at]?.term.buffer.active;
  if (!buffer) return "";
  const lines = Array.from({ length: buffer.length }, (_, row) => buffer.getLine(row)?.translateToString(true) ?? "");
  return lines.join("\n");
}

/**
 * Brings an open pane on screen: in the active pane's place, or at `spot`,
 * "row" or "column" beside or below the active pane, or `{ how, before,
 * target }`, which moves a pane already on screen.
 */
function showPane(pane, spot = "replace") {
  const { how, before = false, target = active } = typeof spot === "string" ? { how: spot } : spot;
  if (typeof spot !== "string" && target !== pane) unplace(pane);
  place(pane, how, before, target);
  active = pane;
  show("shell");
  tick();
}

async function openSession(session, spot = "replace") {
  const open = panes.find((pane) => pane.key === keyOf(session));
  if (open) return showPane(open, spot);
  const pane = addPane(session.title, session.folder, keyOf(session));
  showPane(pane, spot); // A pane is measured only while it is on screen.
  if (await launch(pane, "open", { session: refOf(session) })) follow(pane);
}

/** Splits `from`, or the active pane, with a shell in its folder: to the right ("row") or below ("column"). */
function split(dir, from = active) {
  if (from && shown.includes(from)) active = from;
  openShell(from?.folder, dir);
}

/** Opens the user's own shell in a pane: in `folder`, or where the active pane or the last session ran. */
async function openShell(folder, how = "replace") {
  folder ??= active?.folder ?? state.snapshot.last_folder ?? "~";
  const pane = addPane("Shell", folder, null);
  pane.shell = true;
  showPane(pane, how);
  if (await launch(pane, "shell", { folder })) follow(pane);
}

/** Starts a new session in a domain, under `name` if one is given, and opens it in a pane. */
async function startSession(domain, folder, name) {
  const pane = addPane(name ?? "New session", folder, null);
  showPane(pane);
  const reply = await launch(pane, "start", { domain, folder, name });
  if (!reply) return;
  pane.key = keyOf(reply.session);
  follow(pane);
  await run("snapshot");
  arrange();
}

/** Asks the core to run a process in a pane. Resolves to the reply, or to null when refused. */
async function launch(pane, command, args) {
  try {
    const { rows, cols } = pane.term;
    const reply = await paneCall(command, { ...args, rows, cols });
    [pane.id, pane.born] = [reply.pane, Date.now()];
    return reply;
  } catch (error) {
    toast(error.message);
    closePane(pane);
    return null;
  }
}

function addPane(name, folder, key) {
  const title = el("span", { className: "title", textContent: name, title: "Double-click to rename" });
  const sub = el("span", { className: "sub", textContent: folder ?? "" });
  const body = el("div", { className: "term" });
  const pane = { id: null, key, folder, title, ended: false, heard: Date.now(), sent: Promise.resolve() };
  pane.locate = button("", icon("locate"), "Show on the map", () => locate(pane));
  pane.again = button("", icon("again"), "Run again", () => restart(pane));
  const right = button("", icon("right"), "Split right with a shell (⌘D)", () => split("row", pane));
  const down = button("", icon("down"), "Split down with a shell (⇧⌘D)", () => split("column", pane));
  pane.hide = button("", icon("hide"), "Hide, and keep running", () => hidePane(pane));
  const close = button("close", icon("close"), "Stop and close", () => closePane(pane));
  const head = el("header", {}, title, sub, pane.locate, pane.again, right, down, pane.hide, close);
  title.addEventListener("dblclick", () => rename(title, renamer(pane)));
  carriable(head, () => ({ pane, session: sessionByKey(pane.key) }));
  pane.node = el("div", { className: "pane" }, head, body);
  $("shell").append(pane.node);

  pane.term = new Terminal({
    cursorBlink: true,
    fontFamily: FONT,
    fontSize: 13,
    scrollback: 5000,
    theme: TERMINAL,
  });
  pane.fit = new FitAddon.FitAddon();
  pane.term.loadAddon(pane.fit);
  pane.term.open(body);
  // Drawing on the graphics card keeps a busy terminal smooth. Without it, or
  // when the card takes the context back, the terminal draws as plain elements.
  try {
    const drawer = new WebglAddon.WebglAddon();
    drawer.onContextLoss(() => drawer.dispose());
    pane.term.loadAddon(drawer);
  } catch {
    // Plain elements it is.
  }
  const live = () => pane.id !== null && !pane.ended;

  pane.term.attachCustomKeyEventHandler((event) => !shortcut(event));
  pane.term.onData((data) => {
    // Keystrokes go one after another, so they arrive in the order typed.
    if (live()) pane.sent = pane.sent.then(() => paneCall("write", { pane: pane.id, data })).catch(() => {});
  });
  pane.term.onResize(({ rows, cols }) => {
    if (live()) paneCall("resize", { pane: pane.id, rows, cols }).catch(() => {});
  });
  pane.watch = new ResizeObserver(() => state.view === "shell" && shown.includes(pane) && pane.fit.fit());
  pane.watch.observe(body);
  pane.node.addEventListener("focusin", () => {
    if (active === pane) return;
    active = pane;
    for (const other of panes) other.node.classList.toggle("active", other === pane && shown.length > 1);
  });
  panes.push(pane);
  return pane;
}

/** Copies the pane's output to its terminal until the process ends or the pane closes. */
async function follow(pane) {
  while (panes.includes(pane)) {
    let reply;
    try {
      reply = await paneCall("read", { pane: pane.id });
    } catch {
      return;
    }
    // Closing a pane ends its last read, which then has nothing to show.
    if (!panes.includes(pane)) return;
    if (reply.data) {
      pane.heard = Date.now();
      if (!seen(pane) && pane.heard - pane.born > SETTLE) pane.unseen = true;
      pane.term.write(reply.data);
    }
    if (reply.exited) {
      // A process that ends at once most likely failed, and what it printed says why.
      if (Date.now() - pane.born > LINGER) return closePane(pane);
      pane.ended = true;
      pane.term.write("\r\n\x1b[2mEnded.\x1b[0m\r\n");
      return arrange();
    }
  }
}

function closePane(pane) {
  if (!panes.includes(pane)) return;
  if (pane.id !== null) {
    paneCall("close", { pane: pane.id })
      .catch(() => {})
      .then(() => run("snapshot"));
  }
  pane.watch.disconnect();
  pane.term.dispose();
  pane.node.remove();
  panes.splice(panes.indexOf(pane), 1);
  hidePane(pane);
}

/** Takes a pane off screen. Another open pane takes an empty screen, and the map takes over when none is left. */
function hidePane(pane) {
  unplace(pane);
  if (!layout && panes.length) {
    const other = panes.find((item) => item !== pane);
    if (other) layout = { pane: other };
  }
  if (active === pane || !leafOf(active)) active = leaves(layout)[0] ?? null;
  if (!panes.length && state.view === "shell") show("map");
  else arrange();
}

function restart(pane) {
  const session = sessionByKey(pane.key);
  closePane(pane);
  if (session) openSession(session);
  else if (pane.shell) openShell(pane.folder);
}

function locate(pane) {
  const domain = sessionByKey(pane.key)?.domains[0];
  show("map");
  if (domain !== undefined) focus(domain);
}

// Keys

const SHORTCUTS = {
  1: () => show("map"),
  2: () => show("shell"),
  0: () => fit(),
  b: () => showList(!state.listed),
  d: () => split("row"),
  D: () => split("column"),
  k: () => {
    showList(true);
    $("list-search").focus();
    $("list-search").select();
  },
};

/** What a key press asks of Deck, if anything. A terminal lets these keys through. */
function shortcut(event) {
  const key = event.key.toLowerCase();
  if (!(key in SHORTCUTS)) return null;
  if (event.metaKey) return SHORTCUTS[key === "d" && event.shiftKey ? "D" : key];
  // Control with a letter belongs to the terminal, so elsewhere than macOS the letters take Shift too.
  return event.ctrlKey && (event.shiftKey || key < "a") ? SHORTCUTS[key] : null;
}

document.addEventListener(
  "keydown",
  (event) => {
    const act = shortcut(event);
    if (!act) return;
    event.preventDefault();
    act();
  },
  true,
);

document.addEventListener("keydown", (event) => {
  if (event.key !== "Escape") return;
  if (!$("picker").hidden) closePicker();
  else if (!$("starter").hidden) closeStarter();
  else if (!$("namer").hidden) closeNamer();
  else if (!$("help").hidden) $("help").hidden = true;
  // Inside a terminal the key belongs to the agent.
  else if (document.activeElement.closest(".pane")) return;
  else if (document.activeElement !== document.body) document.activeElement.blur();
  else if (state.view === "map") focus(null);
});

$("show-map").addEventListener("click", () => show("map"));
$("show-shell").addEventListener("click", () => show("shell"));
window.addEventListener("focus", () => {
  run("refresh");
  tick();
});

// A browser tab can go away without closing its panes, so stop their processes.
window.addEventListener("pagehide", () => {
  if (tauri) return;
  for (const pane of panes) {
    if (pane.id === null) continue;
    const request = JSON.stringify({ command: "close", args: { pane: pane.id } });
    navigator.sendBeacon("/pane", new Blob([request], { type: "application/json" }));
  }
});

// Start

// The desktop window on macOS draws under its title bar, where the window's buttons sit.
document.body.classList.toggle("overlaid", Boolean(tauri) && navigator.userAgent.includes("Mac"));
document.fonts.load(`13px ${FONT}`, "\ue0b0").catch(() => {});
setWidth("list", Number(kept("list-width")));
setWidth("panel", Number(kept("panel-width")));
showList(kept("list") !== "hidden");
show("map");
if ((await run("snapshot")) && sky.fresh) fit();
await reopen();
