// Interface for Noema Deck. The core owns every rule; this file sends requests,
// hands each snapshot to the map and fills the lists around it.

import { MapView } from "./map.js";
import { actionFor, label as labelKeys, setMoved } from "./keys.js";
import { setupProfile, showProfile } from "./profile.js";
import { closeSettings, openSettings, setupSettings } from "./settings.js";
import { setupUsage } from "./usage.js";
import { apply, loadThemes, look, watch } from "./theme.js";

const tauri = window.__TAURI__;
// The narrowest, usual and widest a part beside the map can be dragged.
const WIDTHS = { list: [200, 264, 520], panel: [260, 320, 560] };
const QUIET = 3000; // Milliseconds without output before an open session counts as quiet.
const LINGER = 2000; // A pane whose process ends sooner stays up, so what it printed can be read.
const SETTLE = 5000; // Milliseconds a new pane spends drawing itself before its output counts as news.
const REREAD = 10000; // Milliseconds between readings of the agents' sessions while some are open.
const CLICK = 220; // Milliseconds a click on a session waits, in case it is the first of a double click.
const SHOWN = 8; // Sessions a domain in the list shows before it offers the rest.
const $ = (id) => document.getElementById(id);
// Icons are drawn here, sixteen units square, so a button needs no word.
const ICONS = {
  close: "M4 4l8 8M12 4l-8 8",
  hide: "M4 11.5h8",
  right: "M2.5 3.5h11v9h-11zM8 3.5v9",
  down: "M2.5 3.5h11v9h-11zM2.5 8h11",
  again: "M12.4 8.6a4.5 4.5 0 1 1-1.2-3.7M12.6 2.6v2.7H9.9",
  locate: "M8 14s4-3.6 4-7a4 4 0 0 0-8 0c0 3.4 4 7 4 7zM8 8.4a1.4 1.4 0 1 0 0-2.8 1.4 1.4 0 0 0 0 2.8z",
  add: "M8 3.5v9M3.5 8h9",
  fold: "M4.5 6.2l3.5 3.6 3.5-3.6",
  unfoldAll: "M5 6l3-3 3 3M5 10l3 3 3-3",
  folder: "M2 4.6c0-.6.4-1.1 1-1.1h3.2l1.5 1.6H13c.6 0 1 .4 1 1v6.3c0 .6-.4 1.1-1 1.1H3c-.6 0-1-.5-1-1.1z",
  folderOpen: "M2 12.4V4.6c0-.6.4-1.1 1-1.1h3.2l1.5 1.6H12c.6 0 1 .4 1 1v1.1M2 12.4l1.6-4.7c.1-.4.5-.7 1-.7h9.1c.5 0 .8.5.7 1l-1.3 4.4H2z",
  shell: "M4 3h8a2 2 0 0 1 2 2v6a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2zM5 6.5L6.8 8 5 9.5M8.5 10H11",
  foldAll: "M5 3l3 3 3-3M5 13l3-3 3 3",
};

/** The agents a session can run in, by the name the core uses. */
const AGENTS = { claude: "Claude Code", codex: "Codex", antigravity: "Antigravity" };

// Each agent's logo on a 24-unit square, from Simple Icons (CC0). Antigravity's
// is in colour, so the page holds it once as `#logo-antigravity`. The marks
// belong to Anthropic, OpenAI and Google.
const LOGOS = {
  claude: "m4.7144 15.9555 4.7174-2.6471.079-.2307-.079-.1275h-.2307l-.7893-.0486-2.6956-.0729-2.3375-.0971-2.2646-.1214-.5707-.1215-.5343-.7042.0546-.3522.4797-.3218.686.0608 1.5179.1032 2.2767.1578 1.6514.0972 2.4468.255h.3886l.0546-.1579-.1336-.0971-.1032-.0972L6.973 9.8356l-2.55-1.6879-1.3356-.9714-.7225-.4918-.3643-.4614-.1578-1.0078.6557-.7225.8803.0607.2246.0607.8925.686 1.9064 1.4754 2.4893 1.8336.3643.3035.1457-.1032.0182-.0728-.164-.2733-1.3539-2.4467-1.445-2.4893-.6435-1.032-.17-.6194c-.0607-.255-.1032-.4674-.1032-.7285L6.287.1335 6.6997 0l.9957.1336.419.3642.6192 1.4147 1.0018 2.2282 1.5543 3.0296.4553.8985.2429.8318.091.255h.1579v-.1457l.1275-1.706.2368-2.0947.2307-2.6957.0789-.7589.3764-.9107.7468-.4918.5828.2793.4797.686-.0668.4433-.2853 1.8517-.5586 2.9021-.3643 1.9429h.2125l.2429-.2429.9835-1.3053 1.6514-2.0643.7286-.8196.85-.9046.5464-.4311h1.0321l.759 1.1293-.34 1.1657-1.0625 1.3478-.8804 1.1414-1.2628 1.7-.7893 1.36.0729.1093.1882-.0183 2.8535-.607 1.5421-.2794 1.8396-.3157.8318.3886.091.3946-.3278.8075-1.967.4857-2.3072.4614-3.4364.8136-.0425.0304.0486.0607 1.5482.1457.6618.0364h1.621l3.0175.2247.7892.522.4736.6376-.079.4857-1.2142.6193-1.6393-.3886-3.825-.9107-1.3113-.3279h-.1822v.1093l1.0929 1.0686 2.0035 1.8092 2.5075 2.3314.1275.5768-.3218.4554-.34-.0486-2.2039-1.6575-.85-.7468-1.9246-1.621h-.1275v.17l.4432.6496 2.3436 3.5214.1214 1.0807-.17.3521-.6071.2125-.6679-.1214-1.3721-1.9246L14.38 17.959l-1.1414-1.9428-.1397.079-.674 7.2552-.3156.3703-.7286.2793-.6071-.4614-.3218-.7468.3218-1.4753.3886-1.9246.3157-1.53.2853-1.9004.17-.6314-.0121-.0425-.1397.0182-1.4328 1.9672-2.1796 2.9446-1.7243 1.8456-.4128.164-.7164-.3704.0667-.6618.4008-.5889 2.386-3.0357 1.4389-1.882.929-1.0868-.0062-.1579h-.0546l-6.3385 4.1164-1.1293.1457-.4857-.4554.0608-.7467.2307-.2429 1.9064-1.3114Z",
  codex: "M22.2819 9.8211a5.9847 5.9847 0 0 0-.5157-4.9108 6.0462 6.0462 0 0 0-6.5098-2.9A6.0651 6.0651 0 0 0 4.9807 4.1818a5.9847 5.9847 0 0 0-3.9977 2.9 6.0462 6.0462 0 0 0 .7427 7.0966 5.98 5.98 0 0 0 .511 4.9107 6.051 6.051 0 0 0 6.5146 2.9001A5.9847 5.9847 0 0 0 13.2599 24a6.0557 6.0557 0 0 0 5.7718-4.2058 5.9894 5.9894 0 0 0 3.9977-2.9001 6.0557 6.0557 0 0 0-.7475-7.0729zm-9.022 12.6081a4.4755 4.4755 0 0 1-2.8764-1.0408l.1419-.0804 4.7783-2.7582a.7948.7948 0 0 0 .3927-.6813v-6.7369l2.02 1.1686a.071.071 0 0 1 .038.052v5.5826a4.504 4.504 0 0 1-4.4945 4.4944zm-9.6607-4.1254a4.4708 4.4708 0 0 1-.5346-3.0137l.142.0852 4.783 2.7582a.7712.7712 0 0 0 .7806 0l5.8428-3.3685v2.3324a.0804.0804 0 0 1-.0332.0615L9.74 19.9502a4.4992 4.4992 0 0 1-6.1408-1.6464zM2.3408 7.8956a4.485 4.485 0 0 1 2.3655-1.9728V11.6a.7664.7664 0 0 0 .3879.6765l5.8144 3.3543-2.0201 1.1685a.0757.0757 0 0 1-.071 0l-4.8303-2.7865A4.504 4.504 0 0 1 2.3408 7.872zm16.5963 3.8558L13.1038 8.364 15.1192 7.2a.0757.0757 0 0 1 .071 0l4.8303 2.7913a4.4944 4.4944 0 0 1-.6765 8.1042v-5.6772a.79.79 0 0 0-.407-.667zm2.0107-3.0231l-.142-.0852-4.7735-2.7818a.7759.7759 0 0 0-.7854 0L9.409 9.2297V6.8974a.0662.0662 0 0 1 .0284-.0615l4.8303-2.7866a4.4992 4.4992 0 0 1 6.6802 4.66zM8.3065 12.863l-2.02-1.1638a.0804.0804 0 0 1-.038-.0567V6.0742a4.4992 4.4992 0 0 1 7.3757-3.4537l-.142.0805L8.704 5.459a.7948.7948 0 0 0-.3927.6813zm1.0976-2.3654l2.602-1.4998 2.6069 1.4998v2.9994l-2.5974 1.4997-2.6067-1.4997Z",
};

/**
 * Fills a segmented control with a button per agent, the `chosen` one on, and
 * first a button for every agent when `all` names it. `pick` gets the agent
 * clicked, or null for every agent.
 */
function agentChoice(box, chosen, pick, all) {
  const choices = [...(all ? [[null, all]] : []), ...Object.entries(AGENTS)];
  box.replaceChildren(...choices.map(([agent, name]) => {
    const option = el("button", { type: "button" }, ...(agent ? [agentMark(agent)] : []), name);
    option.classList.toggle("on", agent === chosen);
    option.setAttribute("aria-pressed", String(agent === chosen));
    option.addEventListener("click", () => {
      agentChoice(box, agent, pick, all);
      pick(agent);
    });
    return option;
  }));
}

/** The logo of the agent a session runs in, or a terminal window for a shell. */
function agentMark(agent) {
  const artwork = document.getElementById(`logo-${agent}`);
  if (!artwork && !LOGOS[agent]) {
    const mark = icon("shell");
    mark.classList.add("agent", "shell");
    return mark;
  }
  const svg = (name) => document.createElementNS("http://www.w3.org/2000/svg", name);
  const mark = svg("svg");
  mark.setAttribute("viewBox", "0 0 24 24");
  mark.append(artwork ? svg("use") : svg("path"));
  mark.firstChild.setAttribute(artwork ? "href" : "d", artwork ? `#${artwork.id}` : LOGOS[agent]);
  mark.classList.add("agent", "logo", agent);
  return mark;
}

function icon(name) {
  const svg = document.createElementNS("http://www.w3.org/2000/svg", "svg");
  svg.setAttribute("viewBox", "0 0 16 16");
  svg.append(document.createElementNS("http://www.w3.org/2000/svg", "path"));
  svg.firstChild.setAttribute("d", ICONS[name]);
  return svg;
}

export const map = new MapView($("map"));

export const state = {
  snapshot: { domains: [], links: [], sessions: [], missing: [], panes: { open: [], layout: null } },
  view: "map", // "map" or "shell".
  listed: true, // Whether the session list is showing.
  widths: { list: WIDTHS.list[1], panel: WIDTHS.panel[1] },
  focus: null, // ID of the focused domain.
  nested: false, // Whether the panel includes sessions of subdomains.
  query: "",
  picked: new Set(), // Keys of the sessions ticked in the history picker.
  pickAgent: null, // The agent whose sessions the history picker shows, or null for every agent.
  // Domains unfolded in the list, each by its path from a root domain, such as
  // "3/7", since a domain with two parents appears under both.
  unfolded: new Set(),
  more: new Set(), // Paths whose domain shows every one of its sessions.
  folded: new Set(), // Parts of the list folded away: "open", "tree".
};

let drawn = ""; // The snapshot on screen, as the text it arrived in.

// Requests

/**
 * Sends one request down a route: "map" for domains and sessions, "pane" for a
 * terminal, "usage" for the tokens the agents used, "limits" for how much of
 * each plan is used, "fonts" for the installed typefaces.
 */
export async function send(route, request) {
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
  // A new session whose agent has chosen its ID goes by that ID from now on.
  for (const [given, chosen] of snapshot.replaced ?? []) {
    for (const pane of panes.filter((pane) => pane.key === keyOf(given))) {
      pane.key = keyOf(chosen);
      rememberSoon();
    }
  }
  // Most refreshes find nothing new, and then nothing is drawn again.
  const text = JSON.stringify(snapshot);
  if (text !== drawn) {
    [drawn, state.snapshot] = [text, snapshot];
    render();
  }
  if (JSON.stringify(snapshot.appearance) !== JSON.stringify(look.wanted)) await apply(snapshot.appearance);
  return true;
}

// Helpers

const keyOf = (session) => `${session.agent}/${session.id}`;
const refOf = (session) => ({ agent: session.agent, id: session.id });
const domainById = (id) => state.snapshot.domains.find((domain) => domain.id === id);
const sessionByKey = (key) => state.snapshot.sessions.find((session) => keyOf(session) === key);
const matches = (session, query) =>
  `${session.title}\n${session.folder ?? ""}\n${AGENTS[session.agent]}`.toLowerCase().includes(query.trim().toLowerCase());

function ago(seconds) {
  const elapsed = Date.now() / 1000 - seconds;
  if (elapsed < 3600) return `${Math.max(1, Math.round(elapsed / 60))}m`;
  if (elapsed < 86400) return `${Math.round(elapsed / 3600)}h`;
  if (elapsed < 30 * 86400) return `${Math.round(elapsed / 86400)}d`;
  return new Date(seconds * 1000).toLocaleDateString(undefined, { month: "short", day: "numeric" });
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
const fit = () => map.fit(freeLeft(), freeRight());
/** ID of the domain drawn under a screen point, unless something covers the map there. */
const domainUnder = (x, y) => (document.elementFromPoint(x, y) === $("map") ? map.domainAt(x, y) : null);

export function show(view) {
  state.view = view;
  const shell = view === "shell";
  $("shell").hidden = !shell;
  $("show-map").classList.toggle("on", !shell);
  $("show-shell").classList.toggle("on", shell);
  document.body.classList.toggle("on-map", !shell);
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

function focus(id) {
  Object.assign(state, { focus: id, nested: false });
  map.mark({ focus: id });
  renderPanel();
  renderList();
  const domain = domainById(id);
  if (domain) map.bring(domain.x, domain.y, (freeLeft() + freeRight()) / 2, innerHeight / 2);
}

// Drawing

/** Draws a new snapshot. */
function render() {
  const { domains, links } = state.snapshot;
  setMoved(state.snapshot.shortcuts ?? {});
  showProfile(state.snapshot.profile);
  if (state.focus !== null && !domainById(state.focus)) state.focus = null;
  map.chart(domains, links);
  map.mark({ focus: state.focus });
  renderPanel();
  renderList();
  if (!$("picker").hidden) renderPicker();
  $("empty").hidden = domains.length > 0;
  for (const pane of panes) {
    if (!pane.title.querySelector("input")) pane.title.textContent = paneName(pane);
  }
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

// The session list: the open sessions, then every domain as a folder holding
// its subdomains and then its own sessions, most recent first.

/** One row of the list, `depth` steps in. */
function node(className, depth, ...children) {
  const row = el("li", { className: `node ${className}`, tabIndex: 0 }, ...children);
  row.style.setProperty("--depth", depth);
  return row;
}

/**
 * A row for a session, or for a shell, given as a stand-in with only a title
 * and a folder. `pane` is where it is open, if it is. Its one tool closes the
 * pane in the open list, and leaves `domain` in the tree.
 */
function sessionNode(session, pane, depth, domain) {
  // The agent's sign, with a running session's state in its corner.
  const dot = el("span", { className: "dot idle" });
  pane?.dots.push(dot);
  const kind = el("span", { className: "kind" }, agentMark(session.agent), dot);
  const apply = renamer(pane, session.id ? session : undefined);
  const title = el("span", { className: "title", textContent: session.title });
  const meta = el("span", { className: "meta", textContent: session.updated ? ago(session.updated) : "" });
  const tools = el("span", { className: "tools" });
  // In the tree, an empty place where a domain's arrow would be keeps it in line with its sibling domains.
  const slot = domain === undefined ? null : el("span", { className: "mark" });
  const row = node("leaf", depth, slot, kind, title, meta, tools);
  const source = AGENTS[session.agent] ?? "Shell";
  row.title = [session.title, `${source} · ${session.folder ?? "No folder"}`, apply && "Double-click to rename"].filter(Boolean).join("\n");
  if (domain === undefined) tools.append(button("close", icon("close"), "Stop and close", () => closePane(pane)));
  else tools.append(button("close", icon("close"), "Remove from this domain", () => run("unassign", { domain, session: refOf(session) })));
  row.classList.toggle("current", Boolean(pane) && state.view === "shell" && shown.includes(pane));
  const open = () => (pane ? showPane(pane) : openSession(session));
  clicks(row, open, () => rename(title, apply));
  carriable(row, () => (pane ? { pane, session: sessionByKey(pane.key) } : { session }));
  row.addEventListener("keydown", (event) => event.target === row && event.key === "Enter" && open());
  return row;
}

/** How the domains nest, and each one's own sessions, most recent first. */
function treeOf() {
  const { domains, links, sessions } = state.snapshot;
  const byName = (a, b) => a.name.localeCompare(b.name);
  const kids = new Map(domains.map((domain) => [domain.id, []]));
  const below = new Set();
  for (const [parent, child] of links) {
    kids.get(parent)?.push(domainById(child));
    below.add(child);
  }
  for (const list of kids.values()) list.sort(byName);
  const own = new Map(domains.map((domain) => [domain.id, []]));
  for (const session of sessions) for (const id of session.domains) own.get(id)?.push(session);
  for (const list of own.values()) list.sort((a, b) => b.updated - a.updated);
  return { roots: domains.filter((domain) => !below.has(domain.id)).sort(byName), kids, own };
}

/** The path of every domain that holds something to unfold. */
function foldable({ roots, kids, own } = treeOf()) {
  const paths = [];
  const walk = (domain, path) => {
    if (kids.get(domain.id).length || own.get(domain.id).length) paths.push(path);
    for (const child of kids.get(domain.id)) walk(child, `${path}/${child.id}`);
  };
  for (const root of roots) walk(root, `${root.id}`);
  return paths;
}

/** Unfolds or folds the domains at `paths`, and keeps the choice for the next run. */
function unfold(paths, open) {
  for (const path of paths) open ? state.unfolded.add(path) : state.unfolded.delete(path);
  keep("tree", JSON.stringify([...state.unfolded]));
  renderList();
}

/** The rows for a domain and, while it is unfolded, for what it holds. A search unfolds every domain and keeps what matches. */
function domainRows(tree, domain, depth, path) {
  const { kids, own } = tree;
  const searching = Boolean(state.query.trim());
  const inner = kids.get(domain.id).flatMap((child) => domainRows(tree, child, depth + 1, `${path}/${child.id}`));
  const sessions = own.get(domain.id).filter((session) => matches(session, state.query));
  const named = domain.name.toLowerCase().includes(state.query.trim().toLowerCase());
  if (searching && !named && !inner.length && !sessions.length) return [];

  const holds = kids.get(domain.id).length + own.get(domain.id).length > 0;
  const open = searching || state.unfolded.has(path);
  const mark = el("span", { className: "mark" }, holds ? icon("fold") : null);
  const kind = el("span", { className: "kind" }, icon(open && holds ? "folderOpen" : "folder"));
  const title = el("span", { className: "title", textContent: domain.name });
  const count = domain.sessions.length;
  const meta = el("span", { className: "meta", textContent: count ? String(count) : "" });
  const tools = el(
    "span",
    { className: "tools" },
    button("", icon("locate"), "Show on the map", () => {
      show("map");
      focus(domain.id);
    }),
    button("", icon("add"), "New session here", () => openStarter(row, domain.id)),
  );
  const row = node(`domain${open ? "" : " folded"}${count ? "" : " empty"}`, depth, mark, kind, title, meta, tools);
  Object.assign(row.dataset, { domain: domain.id });
  row.setAttribute("role", "treeitem");
  if (holds) row.setAttribute("aria-expanded", String(open));
  row.title = `${domain.name}\nDouble-click to rename`;
  row.classList.toggle("current", state.view === "map" && state.focus === domain.id);
  const toggle = () => holds && !searching && unfold([path], !open);
  row.addEventListener("click", (event) => !event.target.closest("button, input") && toggle());
  row.addEventListener("dblclick", (event) => {
    if (event.target.closest("button, input")) return;
    rename(title, (name) => run("rename_domain", { domain: domain.id, name }));
  });
  row.addEventListener("keydown", (event) => {
    if (event.target !== row) return;
    if (event.key === "Enter" || (event.key === "ArrowRight" && !open) || (event.key === "ArrowLeft" && open)) toggle();
  });
  if (!open) return [row];

  const all = searching || state.more.has(path) || sessions.length <= SHOWN + 1;
  const leaves = (all ? sessions : sessions.slice(0, SHOWN)).map((session) => {
    const pane = panes.find((item) => item.key === keyOf(session));
    return sessionNode(session, pane, depth + 1, domain.id);
  });
  if (!all) {
    const more = node("more", depth + 1, el("span", { className: "mark" }), el("span", { className: "kind" }), el("span", { className: "title", textContent: `${sessions.length - SHOWN} more` }));
    more.addEventListener("click", () => {
      state.more.add(path);
      renderList();
    });
    leaves.push(more);
  }
  return [row, ...inner, ...leaves];
}

function renderList() {
  const { domains } = state.snapshot;
  for (const pane of panes) pane.dots = [];
  const open = panes.flatMap((pane) => {
    const session = sessionByKey(pane.key) ?? { title: paneName(pane), folder: pane.folder, domains: [] };
    return matches(session, state.query) ? [sessionNode(session, pane, 0)] : [];
  });
  const tree = treeOf();
  const rows = tree.roots.flatMap((root) => domainRows(tree, root, 0, `${root.id}`));

  $("open-list").replaceChildren(...open);
  $("tree").replaceChildren(...rows);
  // A folded part keeps its heading, with how much it holds.
  for (const [section, list, count] of [["open", "open-list", open.length], ["tree", "tree", domains.length]]) {
    const folded = state.folded.has(section);
    const head = document.querySelector(`[data-section="${section}"]`);
    $(list).hidden = folded;
    head.setAttribute("aria-expanded", String(!folded));
    head.parentElement.classList.toggle("folded", folded);
    head.querySelector("small").textContent = folded && count ? String(count) : "";
  }
  $("open-head").hidden = !open.length;
  $("tree-head").hidden = !rows.length && !!state.query.trim();
  let note = "";
  if (!domains.length) note = "Double-click the map, or press +, to add a domain.";
  else if (!open.length && !rows.length) note = "Nothing matches.";
  $("list-note").textContent = note;

  // The button unfolds every domain, or folds them all once every one is unfolded.
  const paths = foldable(tree);
  const every = paths.length > 0 && paths.every((path) => state.unfolded.has(path));
  const label = every ? "Fold every domain" : "Unfold every domain";
  $("fold-tree").replaceChildren(icon(every ? "foldAll" : "unfoldAll"));
  Object.assign($("fold-tree"), { title: label });
  $("fold-tree").setAttribute("aria-label", label);
  tick();
}

for (const head of document.querySelectorAll("#list h3 .fold")) {
  head.addEventListener("click", () => {
    const section = head.dataset.section;
    if (!state.folded.delete(section)) state.folded.add(section);
    keep("sections", JSON.stringify([...state.folded]));
    renderList();
  });
}

$("fold-tree").addEventListener("click", () => {
  const paths = foldable();
  const every = paths.every((path) => state.unfolded.has(path));
  state.more.clear();
  unfold(every ? [...state.unfolded] : paths, !every);
});

$("add-root").addEventListener("click", () => {
  const box = $("add-root").getBoundingClientRect();
  const position = map.toWorld((freeLeft() + freeRight()) / 2, innerHeight / 2);
  openNamer(box.right + 8, box.top - 4, (text) => addDomain(text, { position }));
});

/** Whether a pane is where the person can see it. */
const seen = (pane) => state.view === "shell" && shown.includes(pane) && document.hasFocus();

/** Marks each open session as active, quiet, ready or ended. Ready is quiet after output nobody saw. */
function tick() {
  const now = Date.now();
  let ready = false;
  const live = []; // Domains holding a session that printed something just now.
  for (const pane of panes) {
    if (seen(pane)) pane.unseen = false;
    const quiet = now - pane.heard >= QUIET;
    const mood = pane.ended ? "ended" : !quiet ? "active" : pane.unseen ? "ready" : "quiet";
    ready ||= mood === "ready";
    for (const dot of pane.dots ?? []) dot.className = `dot ${mood}`;
    if (mood === "active") live.push(...(sessionByKey(pane.key)?.domains ?? []));
  }
  $("show-shell").classList.toggle("ready", ready);
  map.light(live);
}

setInterval(tick, 1000);

// While sessions run, their agents name them and store new ones, so the list
// reads them again now and then, as it does whenever the window comes back.
setInterval(() => panes.some((pane) => pane.key) && document.hasFocus() && run("refresh"), REREAD);

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
  const folder = el("div", { className: "folder", title: `${AGENTS[session.agent]} · ${session.folder ?? "No folder"}` }, agentMark(session.agent), session.folder ?? "No folder");
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

// Starting a session: choose an agent, a domain and a folder

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
  openStarter.agent = state.snapshot.last_agent ?? "claude";
  agentChoice($("starter-agent"), openStarter.agent, (agent) => (openStarter.agent = agent));

  const box = anchor.getBoundingClientRect();
  $("starter").style.left = `${Math.max(8, Math.min(box.left, innerWidth - 308))}px`;
  $("starter").style.top = `${Math.min(box.bottom + 6, innerHeight - 160)}px`;
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
  startSession(Number($("starter-domain").value), folder, $("starter-name").value.trim() || null, openStarter.agent);
});

document.addEventListener("pointerdown", (event) => {
  if (!event.target.closest("#starter") && event.target !== openStarter.anchor) closeStarter();
});

$("show-settings").addEventListener("click", () => openSettings());
$("show-profile").addEventListener("click", () => openSettings("profile"));

// Adding sessions from the agent's history to the focused domain

function openPicker() {
  state.picked.clear();
  $("picker-search").value = "";
  state.pickAgent = null;
  agentChoice($("picker-agent"), null, (agent) => {
    state.pickAgent = agent;
    renderPicker();
  }, "All");
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
    .filter((session) => !session.domains.includes(domain.id) && (!state.pickAgent || session.agent === state.pickAgent))
    .filter((session) => matches(session, $("picker-search").value))
    .map((session) => {
      const key = keyOf(session);
      const title = el("div", { className: "title", textContent: session.title, title: session.title });
      const when = el("div", { className: "when", textContent: ago(session.updated) });
      const folder = el("div", { className: "folder", title: AGENTS[session.agent] }, agentMark(session.agent), session.folder ?? "No folder");
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

$("map").addEventListener("pointerdown", (event) => {
  if (event.button !== 0) return;
  map.hold();
  const [x, y] = [event.clientX, event.clientY];
  const domain = domainById(map.domainAt(x, y));
  const kind = !domain ? "pan" : event.shiftKey ? "link" : "move";
  drag = { kind, domain, origin: domain && { x: domain.x, y: domain.y }, from: [x, y], x, y, moved: false };
  $("map").setPointerCapture(event.pointerId);
});

$("map").addEventListener("pointermove", (event) => {
  const [x, y] = [event.clientX, event.clientY];
  if (!drag) {
    const hover = map.domainAt(x, y);
    $("map").classList.toggle("over", hover !== null);
    return map.mark({ hover });
  }
  const [dx, dy] = [x - drag.from[0], y - drag.from[1]];
  if (!drag.moved && Math.hypot(dx, dy) < 4) return;
  drag.moved = true;
  $("map").classList.add("dragging");
  if (drag.kind === "pan") {
    map.panBy(x - drag.x, y - drag.y);
  } else if (drag.kind === "move") {
    drag.domain.x = drag.origin.x + dx / map.view.k;
    drag.domain.y = drag.origin.y + dy / map.view.k;
    map.redraw();
  } else {
    map.mark({ rubber: { from: drag.domain.id, x, y }, drop: domainUnder(x, y) });
  }
  [drag.x, drag.y] = [x, y];
});

$("map").addEventListener("pointerup", async (event) => {
  const done = drag;
  drag = null;
  if (!done) return;
  $("map").classList.remove("dragging");
  map.mark({ rubber: null, drop: null });
  if (!done.moved) return focus(done.domain?.id ?? null);
  if (done.kind === "pan") {
    map.release();
  } else if (done.kind === "move") {
    const { id: domain, x, y } = done.domain;
    if (await run("move_domain", { domain, x, y })) return;
    Object.assign(done.domain, done.origin);
    map.redraw();
  } else {
    const child = domainUnder(event.clientX, event.clientY);
    if (child !== null && child !== done.domain.id) run("link", { parent: done.domain.id, child });
  }
});

$("map").addEventListener("pointerleave", () => map.mark({ hover: null }));

$("map").addEventListener("dblclick", (event) => {
  if (map.domainAt(event.clientX, event.clientY) !== null) return;
  const position = map.toWorld(event.clientX, event.clientY);
  openNamer(event.clientX, event.clientY, (text) => addDomain(text, { position }));
});

$("map").addEventListener(
  "wheel",
  (event) => {
    event.preventDefault();
    // A pinch arrives as a wheel with the control key down, in much finer steps.
    const rate = event.ctrlKey ? 0.012 : 0.0018;
    map.zoomAt(event.clientX, event.clientY, Math.exp(-event.deltaY * rate));
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
  if (map.still) return;
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
    map.redraw();
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
    // The empty part of a header along the top drags the window instead. As
    // in Tauri, only the element pressed itself counts, not those around it.
    if (event.button !== 0 || event.target.closest("button, input") || event.target.hasAttribute("data-tauri-drag-region")) return;
    carry = { what: what(), x: event.clientX, y: event.clientY, moved: false, drop: null };
  });
}

const inside = (box, x, y) => x >= box.left && x < box.right && y >= box.top && y < box.bottom;

/** Where a carry would land at a screen point, or null where it cannot. */
function dropAt(x, y) {
  const { pane, session, assignOnly } = carry.what;
  // A domain in the list takes a session in either view.
  const row = session && document.elementFromPoint(x, y)?.closest("#tree .node.domain");
  if (row) return { domain: Number(row.dataset.domain), row };
  if (state.view === "map") {
    const domain = session ? domainUnder(x, y) : null;
    if (domain !== null) return { domain };
    if (assignOnly || document.elementFromPoint(x, y) !== $("map")) return null;
    return { rect: { left: freeLeft(), top: 0, right: freeRight(), bottom: innerHeight } };
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
  map.mark({ drop: drop?.domain ?? null });
  for (const row of document.querySelectorAll("#tree .target")) row.classList.toggle("target", row === drop?.row);
  drop?.row?.classList.add("target");
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
    // The panes along the top stand in for a title bar: the empty part of
    // their headers drags the window, and the one at the left makes room for
    // the window's buttons while the list is hidden.
    const edge = $("shell").getBoundingClientRect();
    for (const pane of shown) {
      const box = pane.node.getBoundingClientRect();
      const top = box.top - edge.top < 2;
      pane.gap.toggleAttribute("data-tauri-drag-region", top);
      pane.node.classList.toggle("corner", top && box.left - edge.left < 2);
    }
    for (const pane of shown) pane.fit.fit();
    active?.term.focus();
  }
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

/** Starts a new session with `agent` in a domain, under `name` if one is given, and opens it in a pane. */
async function startSession(domain, folder, name, agent = "claude") {
  const pane = addPane(name ?? "New session", folder, null, agent);
  showPane(pane);
  const reply = await launch(pane, "start", { domain, agent, folder, name });
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
    const reply = await paneCall(command, { ...args, rows, cols, light: look.mode === "light" });
    [pane.id, pane.born] = [reply.pane, Date.now()];
    return reply;
  } catch (error) {
    toast(error.message);
    closePane(pane);
    return null;
  }
}

function addPane(name, folder, key, agent = key?.split("/")[0]) {
  const mark = agentMark(agent);
  mark.setAttribute("aria-label", AGENTS[agent] ?? "Shell");
  const title = el("span", { className: "title", textContent: name, title: "Double-click to rename" });
  const sub = el("span", { className: "sub", textContent: folder ?? "" });
  const body = el("div", { className: "term" });
  const gap = el("span", { className: "gap" });
  const pane = { id: null, key, folder, title, gap, dots: [], ended: false, heard: Date.now(), sent: Promise.resolve() };
  pane.locate = button("", icon("locate"), "Show on the map", () => locate(pane));
  pane.again = button("", icon("again"), "Run again", () => restart(pane));
  const right = button("", icon("right"), "Split right with a shell", () => split("row", pane));
  const down = button("", icon("down"), "Split down with a shell", () => split("column", pane));
  right.dataset.action = "split_right";
  down.dataset.action = "split_down";
  pane.hide = button("", icon("hide"), "Hide, and keep running", () => hidePane(pane));
  const close = button("close", icon("close"), "Stop and close", () => closePane(pane));
  const head = el("header", {}, mark, title, sub, gap, pane.locate, pane.again, right, down, pane.hide, close);
  title.addEventListener("dblclick", () => rename(title, renamer(pane)));
  carriable(head, () => ({ pane, session: sessionByKey(pane.key) }));
  pane.node = el("div", { className: "pane" }, head, body);
  $("shell").append(pane.node);

  pane.term = new Terminal({ cursorBlink: true, scrollback: 5000, ...look.terminal });
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
  pane.term.onData((data) => live() && type(pane, data));
  // A program can ask to hear when Deck turns light or dark, as Neovim does
  // (mode 2031), and ask which it is now (996). It then reads the colours
  // again, so a running editor follows a change of theme.
  pane.term.parser.registerCsiHandler({ prefix: "?", final: "h" }, (params) => {
    if (params.includes(2031)) pane.hears = true;
    return false;
  });
  pane.term.parser.registerCsiHandler({ prefix: "?", final: "l" }, (params) => {
    if (params.includes(2031)) pane.hears = false;
    return false;
  });
  // A program first asks whether the mode is known (DECRQM), and turns it on only when it is.
  pane.term.parser.registerCsiHandler({ prefix: "?", intermediates: "$", final: "p" }, (params) => {
    if (params[0] !== 2031) return false;
    if (live()) type(pane, `\x1b[?2031;${pane.hears ? 1 : 2}$y`);
    return true;
  });
  pane.term.parser.registerCsiHandler({ prefix: "?", final: "n" }, (params) => {
    if (params[0] !== 996) return false;
    if (live()) type(pane, scheme());
    return true;
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
  labelKeys();
  return pane;
}

/** Sends text to a pane's process as if typed. Keystrokes go one after another, so they arrive in the order typed. */
function type(pane, data) {
  pane.sent = pane.sent.then(() => paneCall("write", { pane: pane.id, data })).catch(() => {});
}

/** The report that says whether Deck is dark (1) or light (2). */
const scheme = () => `\x1b[?997;${look.mode === "dark" ? 1 : 2}n`;

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

/** What each shortcut does. `keys.js` says which key each is on. */
const SHORTCUTS = {
  map: () => show("map"),
  shell: () => show("shell"),
  fit: () => fit(),
  list: () => showList(!state.listed),
  split_right: () => split("row"),
  split_down: () => split("column"),
  settings: () => ($("settings").hidden ? openSettings() : closeSettings()),
  search: () => {
    showList(true);
    $("list-search").focus();
    $("list-search").select();
  },
};

/** What a key press asks of Deck, if anything. A terminal lets these keys through. */
const shortcut = (event) => SHORTCUTS[actionFor(event)] ?? null;

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
  if (!$("settings").hidden) closeSettings();
  else if (!$("picker").hidden) closePicker();
  else if (!$("starter").hidden) closeStarter();
  else if (!$("namer").hidden) closeNamer();
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
// A new look repaints the map and restyles every terminal.
let shownMode = null; // Light or dark, as the terminals were last told.
watch(({ painter, terminal, mode }) => {
  map.paintWith(painter);
  for (const pane of panes) {
    Object.assign(pane.term.options, terminal);
    // The graphics card keeps each glyph drawn in its colours, which a new
    // palette or contrast leaves stale until they are drawn again.
    pane.term.clearTextureAtlas();
    pane.fit.fit();
    if (pane.hears && shownMode && mode !== shownMode && pane.id !== null && !pane.ended) type(pane, scheme());
  }
  shownMode = mode;
});
setupSettings({
  appearance: (appearance) => run("set_appearance", { appearance }),
  shortcuts: (shortcuts) => run("set_shortcuts", { shortcuts }),
  fonts: () => send("fonts", {}),
});
setupProfile((profile) => run("set_profile", { profile }));
setupUsage({
  usage: () => send("usage", {}),
  limits: (request) => send("limits", request),
  agents: AGENTS,
  agentMark,
  session: sessionByKey,
  open: (session) => {
    closeSettings();
    openSession(session);
  },
});
setWidth("list", Number(kept("list-width")));
setWidth("panel", Number(kept("panel-width")));
showList(kept("list") !== "hidden");
try {
  state.unfolded = new Set(JSON.parse(kept("tree")) ?? []);
  state.folded = new Set(JSON.parse(kept("sections")) ?? []);
} catch {
  // A list kept by an older run starts as it is.
}
labelKeys();
show("map");
await loadThemes();
if ((await run("snapshot")) && map.fresh) fit();
await reopen();
