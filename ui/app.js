// Map interface for Noema Deck. The core owns every rule; this file sends
// requests, hands the snapshot that comes back to the sky and fills the panels.

import { Sky } from "./sky.js";

const tauri = window.__TAURI__;
const TRAY = 314; // Width the unsorted tray takes from the left edge.
const PANEL = 372; // Width the star panel takes from the right edge.
const $ = (id) => document.getElementById(id);

export const sky = new Sky($("sky"));

export const state = {
  snapshot: { stars: [], links: [], sessions: [], missing: [] },
  focus: null, // ID of the focused star.
  starting: false, // Whether the focused star's new-session form is open.
  withSmaller: false,
  selected: new Set(), // Keys of the unsorted sessions picked for one drag.
  query: "",
};

let drawn = ""; // The snapshot on screen, as the text it arrived in.

// Requests

/** Sends one request down a route: "map" for the universe, "pane" for a shell pane. */
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
const starById = (id) => state.snapshot.stars.find((star) => star.id === id);

function ago(seconds) {
  const elapsed = Date.now() / 1000 - seconds;
  if (elapsed < 3600) return `${Math.max(1, Math.round(elapsed / 60))}m ago`;
  if (elapsed < 86400) return `${Math.round(elapsed / 3600)}h ago`;
  if (elapsed < 30 * 86400) return `${Math.round(elapsed / 86400)}d ago`;
  return new Date(seconds * 1000).toLocaleDateString();
}

function el(tag, props = {}, ...children) {
  const node = Object.assign(document.createElement(tag), props);
  node.append(...children);
  return node;
}

function toast(message) {
  $("toast").textContent = message;
  $("toast").hidden = false;
  clearTimeout(toast.timer);
  toast.timer = setTimeout(() => ($("toast").hidden = true), 5000);
}

// The map

/** Right edge of the sky that the panel leaves free. */
const freeRight = () => innerWidth - (state.focus === null ? 0 : PANEL);
const fit = () => sky.fit(TRAY, freeRight());
/** ID of the star under a screen point, unless a panel covers it there. */
const starUnder = (x, y) => (document.elementFromPoint(x, y) === $("sky") ? sky.starAt(x, y) : null);

function focus(id) {
  Object.assign(state, { focus: id, withSmaller: false, starting: false });
  sky.mark({ focus: id });
  renderPanel();
  const star = starById(id);
  if (star) sky.bring(star.x, star.y, (TRAY + freeRight()) / 2, innerHeight / 2);
}

// Drawing

/** Draws a new snapshot. */
function render() {
  const { stars, links, sessions } = state.snapshot;
  if (state.focus !== null && !starById(state.focus)) state.focus = null;
  sky.chart(stars, links);
  sky.mark({ focus: state.focus });
  renderPanel();
  renderTray();
  $("empty").hidden = stars.length > 0;
  for (const pane of panes) {
    const session = sessions.find((session) => keyOf(session) === pane.key);
    if (session) pane.title.textContent = session.title;
  }
}

function sessionRow(session, starId) {
  const title = el("div", { className: "title", textContent: session.title, title: session.title });
  const open = el("button", { className: "open", type: "button", textContent: "Open" });
  open.addEventListener("click", () => openSession(session));
  const tools = el("div", { className: "tools" }, open);
  if (session.stars.includes(starId)) tools.append(removeButton(starId, session));
  const folder = el("div", {
    className: "folder",
    textContent: session.folder ?? "No folder",
    title: session.folder ?? "",
  });
  const when = el("div", { className: "when", textContent: ago(session.updated) });
  const row = el("li", { className: "session" }, title, tools, folder, when);

  const others = session.stars.filter((id) => id !== starId).map((id) => starById(id)?.name);
  if (others.length) {
    const chips = others.map((name) => el("span", { className: "chip plain", textContent: name }));
    row.append(el("div", { className: "chips" }, ...chips));
  }
  row.addEventListener("pointerdown", (event) => {
    if (event.button !== 0 || event.target.closest("button")) return;
    carry = { session, fromTray: starId === null, x: event.clientX, y: event.clientY, keys: null };
  });
  return row;
}

function removeButton(star, session) {
  const button = el("button", { className: "remove", type: "button", textContent: "×" });
  button.setAttribute("aria-label", "Remove from this star");
  button.addEventListener("click", () => {
    run("unplace", { star, session: { agent: session.agent, id: session.id } });
  });
  return button;
}

function linkRow(label, stars, unlink) {
  const chips = stars.map((star) => {
    const go = el("button", { className: "go", type: "button", textContent: star.name });
    const drop = el("button", { className: "drop", type: "button", textContent: "×" });
    go.addEventListener("click", () => focus(star.id));
    drop.setAttribute("aria-label", `Unlink ${star.name}`);
    drop.addEventListener("click", () => unlink(star.id));
    return el("span", { className: "chip" }, go, drop);
  });
  return el("div", { className: "link-row" }, label, ...chips);
}

function renderPanel() {
  const star = starById(state.focus);
  $("panel").hidden = !star;
  document.body.classList.toggle("focused", Boolean(star));
  if (!star) return;
  const { links, sessions, missing } = state.snapshot;
  if (document.activeElement !== $("star-name")) $("star-name").value = star.name;
  $("delete-star").textContent = "Delete star";
  $("with-smaller").checked = state.withSmaller;
  $("starter").hidden = !state.starting;

  const larger = links.filter(([, smaller]) => smaller === star.id).map(([id]) => starById(id));
  const smaller = links.filter(([id]) => id === star.id).map(([, id]) => starById(id));
  const rows = [];
  if (larger.length) {
    rows.push(linkRow("Under", larger, (id) => run("unlink", { larger: id, smaller: star.id })));
  }
  if (smaller.length) {
    rows.push(linkRow("Smaller", smaller, (id) => run("unlink", { larger: star.id, smaller: id })));
  }
  $("star-links").replaceChildren(...rows);

  const beneath = new Set(star.sessions.map(keyOf));
  const here = (session) => session.stars.includes(star.id);
  const shown = state.withSmaller ? (session) => beneath.has(keyOf(session)) : here;
  const count = sessions.filter(here).length + missing.filter(here).length;
  $("star-meta").textContent =
    star.sessions.length === count
      ? `${count} ${count === 1 ? "session" : "sessions"}`
      : `${count} here, ${star.sessions.length} with smaller stars`;
  const gone = missing.filter(shown).map((session) => {
    const text = `Missing session ${session.id.slice(0, 8)}`;
    const row = el("li", { className: "session missing" }, el("div", { className: "title", textContent: text }));
    if (here(session)) row.append(el("div", { className: "tools" }, removeButton(star.id, session)));
    return row;
  });
  $("star-sessions").replaceChildren(
    ...sessions.filter(shown).map((session) => sessionRow(session, star.id)),
    ...gone,
  );
}

function renderTray() {
  const unsorted = state.snapshot.sessions.filter((session) => session.stars.length === 0);
  const keys = new Set(unsorted.map(keyOf));
  for (const key of state.selected) if (!keys.has(key)) state.selected.delete(key);
  const query = state.query.toLowerCase();
  const shown = unsorted.filter((session) =>
    `${session.title}\n${session.folder ?? ""}`.toLowerCase().includes(query),
  );
  $("tray-count").textContent = unsorted.length;
  $("tray-list").replaceChildren(
    ...shown.map((session) => {
      const row = sessionRow(session, null);
      row.classList.toggle("selected", state.selected.has(keyOf(session)));
      return row;
    }),
  );
  let note = "Drag a session onto a star. Click to pick several.";
  if (!unsorted.length) note = "Every session sits on a star.";
  else if (!shown.length) note = "No session matches.";
  $("tray-note").textContent = note;
}

// Naming a new star

/** Adds one star, or a folder's tree of stars when the text is a folder's path. */
async function addStar(text, where) {
  if (!text.startsWith("/") && !text.startsWith("~")) return run("add_star", { name: text, ...where });
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
  if (!name) return toast("Enter a name for the star.");
  closeNamer();
  onName(name);
});
$("namer-input").addEventListener("blur", closeNamer);

// The sky: pan, zoom, move, focus and link

let drag = null;

$("sky").addEventListener("pointerdown", (event) => {
  if (event.button !== 0) return;
  sky.hold();
  const [x, y] = [event.clientX, event.clientY];
  const star = starById(sky.starAt(x, y));
  const kind = !star ? "pan" : event.shiftKey ? "link" : "move";
  drag = { kind, star, origin: star && { x: star.x, y: star.y }, from: [x, y], x, y, moved: false };
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
    drag.star.x = drag.origin.x + dx / sky.view.k;
    drag.star.y = drag.origin.y + dy / sky.view.k;
    sky.redraw();
  } else {
    sky.mark({ rubber: { from: drag.star.id, x, y }, drop: starUnder(x, y) });
  }
  [drag.x, drag.y] = [x, y];
});

$("sky").addEventListener("pointerup", async (event) => {
  const done = drag;
  drag = null;
  if (!done) return;
  $("sky").classList.remove("dragging");
  sky.mark({ rubber: null, drop: null });
  if (!done.moved) return focus(done.star?.id ?? null);
  if (done.kind === "pan") {
    sky.release();
  } else if (done.kind === "move") {
    const { id: star, x, y } = done.star;
    if (await run("move_star", { star, x, y })) return;
    Object.assign(done.star, done.origin);
    sky.redraw();
  } else {
    const smaller = starUnder(event.clientX, event.clientY);
    if (smaller !== null && smaller !== done.star.id) run("link", { larger: done.star.id, smaller });
  }
});

$("sky").addEventListener("pointerleave", () => sky.mark({ hover: null }));

$("sky").addEventListener("dblclick", (event) => {
  if (sky.starAt(event.clientX, event.clientY) !== null) return;
  const position = sky.toWorld(event.clientX, event.clientY);
  openNamer(event.clientX, event.clientY, (text) => addStar(text, { position }));
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

// Carrying sessions from a list onto a star

let carry = null;

document.addEventListener("pointermove", (event) => {
  if (!carry) return;
  if (!carry.keys) {
    if (Math.hypot(event.clientX - carry.x, event.clientY - carry.y) < 5) return;
    const key = keyOf(carry.session);
    carry.keys = carry.fromTray && state.selected.has(key) ? [...state.selected] : [key];
    $("ghost").textContent =
      carry.keys.length === 1 ? carry.session.title : `${carry.keys.length} sessions`;
    $("ghost").hidden = false;
  }
  $("ghost").style.left = `${event.clientX + 12}px`;
  $("ghost").style.top = `${event.clientY + 12}px`;
  sky.mark({ drop: starUnder(event.clientX, event.clientY) });
});

document.addEventListener("pointerup", async (event) => {
  const done = carry;
  carry = null;
  if (!done) return;
  $("ghost").hidden = true;
  sky.mark({ drop: null });
  if (!done.keys) {
    if (!done.fromTray) return;
    const key = keyOf(done.session);
    if (!state.selected.delete(key)) state.selected.add(key);
    return renderTray();
  }
  const star = starUnder(event.clientX, event.clientY);
  if (star === null) return;
  const sessions = done.keys.map((key) => {
    const [agent, id] = key.split("/");
    return { agent, id };
  });
  if (await run("place", { star, sessions })) {
    state.selected.clear();
    renderTray();
  }
});

// Panel and tray controls

$("star-name").addEventListener("change", async () => {
  const renamed = await run("rename_star", { star: state.focus, name: $("star-name").value });
  if (!renamed) $("star-name").value = starById(state.focus)?.name ?? "";
});

$("star-name").addEventListener("keydown", (event) => {
  if (event.key === "Enter") $("star-name").blur();
});

$("add-smaller").addEventListener("click", () => {
  const box = $("add-smaller").getBoundingClientRect();
  const larger = state.focus;
  openNamer(box.left, box.bottom + 6, (text) => addStar(text, { larger }));
});

$("new-session").addEventListener("click", () => {
  state.starting = !state.starting;
  renderPanel();
  if (!state.starting) return;
  const { sessions, last_folder: last } = state.snapshot;
  const folders = [...new Set(sessions.map((session) => session.folder).filter(Boolean))];
  $("folders").replaceChildren(...folders.map((value) => el("option", { value })));
  $("starter-folder").value = last ?? folders[0] ?? "~";
  $("starter-folder").focus();
  $("starter-folder").select();
});

$("starter").addEventListener("submit", (event) => {
  event.preventDefault();
  const folder = $("starter-folder").value.trim();
  if (!folder) return toast("Enter a folder for the session.");
  startSession(state.focus, folder);
});

$("delete-star").addEventListener("click", () => {
  if ($("delete-star").textContent === "Delete star") {
    $("delete-star").textContent = "Confirm delete";
  } else {
    run("delete_star", { star: state.focus });
  }
});

$("with-smaller").addEventListener("change", () => {
  state.withSmaller = $("with-smaller").checked;
  renderPanel();
});

$("tray-search").addEventListener("input", () => {
  state.query = $("tray-search").value;
  renderTray();
});

document.addEventListener("keydown", (event) => {
  // In the shell view every key but the view switch belongs to the terminal.
  if (event.key !== "Escape" || !$("shell").hidden) return;
  if (!$("namer").hidden) closeNamer();
  else if (document.activeElement === document.body) focus(null);
  else document.activeElement.blur();
});

window.addEventListener("focus", () => run("refresh"));

// Shell panes: each shows the agent's own interface for one session

const panes = []; // Open panes from left to right.
let active = null; // The pane that last had the keyboard.

const viewKey = (event) => (event.metaKey || event.ctrlKey) && ["1", "2"].includes(event.key);

function show(view) {
  const shell = view === "shell" && panes.length > 0;
  $("shell").hidden = !shell;
  $("views").hidden = panes.length === 0;
  $("show-map").classList.toggle("on", !shell);
  $("show-shell").classList.toggle("on", shell);
  if (!shell) return document.activeElement?.blur();
  for (const pane of panes) {
    pane.node.classList.toggle("active", pane === active);
    pane.fit.fit();
  }
  active?.term.focus();
}

async function openSession(session) {
  let pane = panes.find((pane) => pane.key === keyOf(session));
  if (!pane) {
    if (panes.length === 2) return toast("Two panes are open. Close one to open another.");
    pane = addPane(session.title, session.folder, keyOf(session));
    if (!(await launch(pane, "open", { session: { agent: session.agent, id: session.id } }))) return;
    follow(pane);
  }
  active = pane;
  show("shell");
}

/** Starts a new session on a star and opens it in a pane. */
async function startSession(star, folder) {
  if (panes.length === 2) return toast("Two panes are open. Close one to start a session.");
  const pane = addPane("New session", folder, null);
  const reply = await launch(pane, "start", { star, folder });
  if (!reply) return;
  pane.key = keyOf(reply.session);
  state.starting = false;
  follow(pane);
  run("snapshot");
}

/** Shows a new pane and asks the core to run a process in it. Resolves to the reply, or to null when refused. */
async function launch(pane, command, args) {
  active = pane;
  show("shell"); // A pane is measured only while it is on screen.
  try {
    const { rows, cols } = pane.term;
    const reply = await paneCall(command, { ...args, rows, cols });
    pane.id = reply.pane;
    return reply;
  } catch (error) {
    toast(error.message);
    closePane(pane);
    return null;
  }
}

function addPane(name, folder, key) {
  const label = "Close this pane and stop Claude Code";
  const close = el("button", { className: "close", type: "button", textContent: "×", title: label });
  close.setAttribute("aria-label", label);
  const title = el("span", { className: "title", textContent: name });
  const sub = el("span", { className: "sub", textContent: folder ?? "" });
  const body = el("div", { className: "term" });
  const node = el("div", { className: "pane" }, el("header", {}, title, sub, close), body);
  $("shell").append(node);

  const term = new Terminal({
    cursorBlink: true,
    fontFamily: 'ui-monospace, "SF Mono", Menlo, Consolas, monospace',
    fontSize: 13,
    scrollback: 5000,
    theme: {
      background: "#090b16",
      foreground: "#f4f0e6",
      cursor: "#f1cc7a",
      selectionBackground: "rgba(150, 170, 225, 0.35)",
    },
  });
  const fit = new FitAddon.FitAddon();
  term.loadAddon(fit);
  term.open(body);
  const pane = { id: null, key, node, title, term, fit, ended: false, sent: Promise.resolve() };
  const live = () => pane.id !== null && !pane.ended;

  term.attachCustomKeyEventHandler((event) => !viewKey(event));
  term.onData((data) => {
    // Keystrokes go one after another, so they arrive in the order typed.
    if (live()) pane.sent = pane.sent.then(() => paneCall("write", { pane: pane.id, data })).catch(() => {});
  });
  term.onResize(({ rows, cols }) => {
    if (live()) paneCall("resize", { pane: pane.id, rows, cols }).catch(() => {});
  });
  pane.watch = new ResizeObserver(() => $("shell").hidden || fit.fit());
  pane.watch.observe(body);
  node.addEventListener("focusin", () => {
    active = pane;
    for (const other of panes) other.node.classList.toggle("active", other === pane);
  });
  close.addEventListener("click", () => closePane(pane));
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
    if (reply.data) pane.term.write(reply.data);
    if (reply.exited) {
      pane.ended = true;
      return pane.term.write("\r\n\x1b[2mClaude Code has stopped. Close this pane.\x1b[0m\r\n");
    }
  }
}

function closePane(pane) {
  if (pane.id !== null) {
    paneCall("close", { pane: pane.id })
      .catch(() => {})
      .then(() => run("snapshot"));
  }
  pane.watch.disconnect();
  pane.term.dispose();
  pane.node.remove();
  panes.splice(panes.indexOf(pane), 1);
  if (active === pane) active = panes[0] ?? null;
  show(panes.length ? "shell" : "map");
}

$("show-map").addEventListener("click", () => show("map"));
$("show-shell").addEventListener("click", () => show("shell"));

document.addEventListener(
  "keydown",
  (event) => {
    if (!viewKey(event)) return;
    event.preventDefault();
    show(event.key === "1" ? "map" : "shell");
  },
  true,
);

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

if ((await run("snapshot")) && sky.fresh) fit();
