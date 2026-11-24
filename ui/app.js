// Interface for Noema Deck. The core owns every rule; this file sends requests,
// hands each snapshot to the sky and fills the lists around it.

import { Sky } from "./sky.js";

const tauri = window.__TAURI__;
const RAIL = 44; // Width of the view rail on the left edge.
// The narrowest, usual and widest a part beside the map can be dragged.
const WIDTHS = { list: [180, 248, 520], panel: [260, 320, 560] };
const QUIET = 3000; // Milliseconds without output before an open session counts as quiet.
const LINGER = 2000; // A pane whose process ends sooner stays up, so what it printed can be read.
const RECENT = 40; // Sessions the list shows before a search narrows it.
const FONT = 'ui-monospace, "SF Mono", Menlo, Consolas, "Symbols Nerd Font Mono", monospace';
const $ = (id) => document.getElementById(id);

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
  snapshot: { domains: [], links: [], sessions: [], missing: [] },
  view: "map", // "map" or "shell".
  listed: true, // Whether the session list is showing.
  widths: { list: WIDTHS.list[1], panel: WIDTHS.panel[1] },
  stacked: false, // Whether two panes sit one above the other and not side by side.
  ratio: 0.5, // The share of the terminal the first of two panes takes.
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

function toast(message) {
  $("toast").textContent = message;
  $("toast").hidden = false;
  clearTimeout(toast.timer);
  toast.timer = setTimeout(() => ($("toast").hidden = true), 5000);
}

// Views: the map or the terminals fill the window, with the session list beside either

const freeLeft = () => RAIL + (state.listed ? state.widths.list : 0);
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
  renderPanel();
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

/** Sets the share of the terminal the first of two panes takes, or puts it back to half. */
function setRatio(share) {
  state.ratio = Math.min(0.8, Math.max(0.2, share || 0.5));
  keep("ratio", state.ratio);
  arrange();
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
dragging($("list-edge"), (event) => setWidth("list", event.clientX - RAIL));
dragging($("panel-edge"), (event) => setWidth("panel", innerWidth - event.clientX));
dragging($("divider"), (event) => {
  const box = $("shell").getBoundingClientRect();
  setRatio(state.stacked ? (event.clientY - box.top) / box.height : (event.clientX - box.left) / box.width);
});
$("list-edge").addEventListener("dblclick", () => setWidth("list"));
$("panel-edge").addEventListener("dblclick", () => setWidth("panel"));
$("divider").addEventListener("dblclick", () => setRatio());

function focus(id) {
  Object.assign(state, { focus: id, nested: false });
  sky.mark({ focus: id });
  renderPanel();
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
    const session = sessionByKey(pane.key);
    if (session) pane.title.textContent = session.title;
  }
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
  const text = el("span", { className: "title", textContent: title, title });
  const tools = el("span", { className: "tools" });
  const row = el("li", { className: "entry", tabIndex: 0 }, dot, text, el("span", { className: "sub", textContent: sub }), tools);
  const open = (beside) => (pane ? showPane(pane, beside) : openSession(session, beside));
  tools.append(button("", icon("right"), "Open beside", () => open(true)));
  if (pane) {
    pane.dot = dot;
    tools.append(button("close", icon("close"), "Stop and close", () => closePane(pane)));
    row.classList.toggle("current", state.view === "shell" && shown.includes(pane));
  }
  row.addEventListener("click", () => open(false));
  row.addEventListener("keydown", (event) => event.key === "Enter" && open(false));
  return row;
}

function renderList() {
  const { sessions } = state.snapshot;
  const describe = (session) => {
    const domain = domainById(session.domains[0])?.name;
    return [domain, ago(session.updated)].filter(Boolean).join(", ");
  };
  const open = panes.flatMap((pane) => {
    const session = sessionByKey(pane.key) ?? { title: pane.title.textContent, folder: pane.folder, domains: [] };
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

/** Marks each open session as active, quiet or ended. */
function tick() {
  const now = Date.now();
  for (const pane of panes) {
    const mood = pane.ended ? "ended" : now - pane.heard < QUIET ? "active" : "quiet";
    if (pane.dot) pane.dot.className = `dot ${mood}`;
  }
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
  const title = el("div", { className: "title", textContent: session.title, title: session.title });
  const tools = el("div", { className: "tools" }, button("open", "Open", "Open in a terminal", () => openSession(session)));
  if (session.domains.includes(domain)) tools.append(removeButton(domain, session));
  const folder = el("div", { className: "folder", textContent: session.folder ?? "No folder", title: session.folder ?? "" });
  const when = el("div", { className: "when", textContent: ago(session.updated) });
  const row = el("li", { className: "session" }, title, tools, folder, when, chips(session, domain));
  row.addEventListener("pointerdown", (event) => {
    if (event.button !== 0 || event.target.closest("button")) return;
    carry = { session, x: event.clientX, y: event.clientY, moved: false };
  });
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

  const box = anchor.getBoundingClientRect();
  $("starter").style.left = `${Math.max(8, Math.min(box.left, innerWidth - 308))}px`;
  $("starter").style.top = `${Math.min(box.bottom + 6, innerHeight - 124)}px`;
  $("starter").hidden = false;
  $("starter-folder").focus();
  $("starter-folder").select();
}

function closeStarter() {
  $("starter").hidden = true;
}

$("starter").addEventListener("submit", (event) => {
  event.preventDefault();
  const folder = $("starter-folder").value.trim();
  if (!folder) return toast("Enter a folder for the session.");
  closeStarter();
  startSession(Number($("starter-domain").value), folder);
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

// Carrying a session from the panel onto another domain

let carry = null;

document.addEventListener("pointermove", (event) => {
  if (!carry) return;
  if (!carry.moved) {
    if (Math.hypot(event.clientX - carry.x, event.clientY - carry.y) < 5) return;
    carry.moved = true;
    $("ghost").textContent = carry.session.title;
    $("ghost").hidden = false;
  }
  $("ghost").style.left = `${event.clientX + 12}px`;
  $("ghost").style.top = `${event.clientY + 12}px`;
  sky.mark({ drop: domainUnder(event.clientX, event.clientY) });
});

document.addEventListener("pointerup", (event) => {
  const done = carry;
  carry = null;
  if (!done?.moved) return;
  $("ghost").hidden = true;
  sky.mark({ drop: null });
  const domain = domainUnder(event.clientX, event.clientY);
  if (domain !== null) run("assign", { domain, sessions: [refOf(done.session)] });
});

// Terminals: each runs the agent's own interface for one session, or a shell

const panes = []; // Every open pane, in the order opened.
const shown = []; // The one or two panes on screen, from left to right.
let active = null; // The pane that last had the keyboard.

/** Puts the terminals on screen: the shown panes side by side, the rest kept running out of sight. */
function arrange() {
  $("shell").classList.toggle("stacked", state.stacked);
  $("divider").hidden = shown.length < 2;
  for (const pane of panes) {
    const at = shown.indexOf(pane);
    pane.node.hidden = at < 0;
    // The divider sits at 1, between the two panes.
    pane.node.style.order = at * 2;
    pane.node.style.flexGrow = shown.length < 2 ? 1 : at ? 1 - state.ratio : state.ratio;
    pane.node.classList.toggle("first", at === 0);
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
  renderList();
}

/** The text on screen in one of the shown panes, for the self-test. */
export function screenText(at = 0) {
  const buffer = shown[at]?.term.buffer.active;
  if (!buffer) return "";
  const lines = Array.from({ length: buffer.length }, (_, row) => buffer.getLine(row)?.translateToString(true) ?? "");
  return lines.join("\n");
}

/** Brings an open pane on screen, in the active pane's place or beside it. */
function showPane(pane, beside = false) {
  if (!shown.includes(pane)) {
    const at = Math.max(0, shown.indexOf(active));
    if (beside && shown.length === 2) shown[1 - at] = pane;
    else if (beside || !shown.length) shown.push(pane);
    else shown[at] = pane;
  }
  active = pane;
  show("shell");
}

async function openSession(session, beside = false) {
  const open = panes.find((pane) => pane.key === keyOf(session));
  if (open) return showPane(open, beside);
  const pane = addPane(session.title, session.folder, keyOf(session));
  showPane(pane, beside); // A pane is measured only while it is on screen.
  if (await launch(pane, "open", { session: refOf(session) })) follow(pane);
}

/** Splits the terminal with a shell in the active pane's folder, to the right or below. */
function split(stacked, folder) {
  state.stacked = stacked;
  openShell(folder, true);
}

/** Opens the user's own shell in a pane: in `folder`, or where the active pane or the last session ran. */
async function openShell(folder, beside = false) {
  folder ??= active?.folder ?? state.snapshot.last_folder ?? "~";
  const pane = addPane("Shell", folder, null);
  pane.shell = true;
  showPane(pane, beside);
  if (await launch(pane, "shell", { folder })) follow(pane);
}

/** Starts a new session in a domain and opens it in a pane. */
async function startSession(domain, folder) {
  const pane = addPane("New session", folder, null);
  showPane(pane);
  const reply = await launch(pane, "start", { domain, folder });
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
  const title = el("span", { className: "title", textContent: name });
  const sub = el("span", { className: "sub", textContent: folder ?? "" });
  const body = el("div", { className: "term" });
  const pane = { id: null, key, folder, title, ended: false, heard: Date.now(), sent: Promise.resolve() };
  pane.locate = button("", icon("locate"), "Show on the map", () => locate(pane));
  pane.again = button("", icon("again"), "Run again", () => restart(pane));
  const right = button("", icon("right"), "Split right with a shell (⌘D)", () => split(false, pane.folder));
  const down = button("", icon("down"), "Split down with a shell (⇧⌘D)", () => split(true, pane.folder));
  pane.hide = button("", icon("hide"), "Hide, and keep running", () => hidePane(pane));
  const close = button("close", icon("close"), "Stop and close", () => closePane(pane));
  const head = el("header", {}, title, sub, pane.locate, pane.again, right, down, pane.hide, close);
  // The window is dragged by a pane's header, which lies along its top edge.
  for (const part of [head, title, sub]) part.setAttribute("data-tauri-drag-region", "");
  pane.node = el("div", { className: "pane" }, head, body);
  $("shell").append(pane.node);

  pane.term = new Terminal({
    cursorBlink: true,
    fontFamily: FONT,
    fontSize: 13,
    scrollback: 5000,
    theme: {
      background: "#090b16",
      foreground: "#f4f0e6",
      cursor: "#f1cc7a",
      selectionBackground: "rgba(150, 170, 225, 0.35)",
    },
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
  if (shown.includes(pane)) shown.splice(shown.indexOf(pane), 1);
  if (!shown.length && panes.length) shown.push(panes.find((other) => other !== pane) ?? panes[0]);
  if (active === pane || !shown.includes(active)) active = shown[0] ?? null;
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
  b: () => showList(!state.listed),
  d: () => split(false),
  D: () => split(true),
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
window.addEventListener("focus", () => run("refresh"));

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
state.ratio = Math.min(0.8, Math.max(0.2, Number(kept("ratio")) || 0.5));
show("map");
if ((await run("snapshot")) && sky.fresh) fit();
