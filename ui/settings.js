// Settings: a page for each part, in the manner of Claude's desktop app. The
// look (theme, colour mode, typefaces) shows each change at once, and the core
// keeps it; the other pages list the shortcuts and what this copy of Deck is.

import { ACTIONS, display, keyOf, pressed, setMoved } from "./keys.js";
import { apply, look, modeOf, themes, watch } from "./theme.js";

const $ = (id) => document.getElementById(id);
// What each font menu offers and where its choice is kept: every installed
// family for the interface, and those whose letters share one width for terminals.
const MENUS = {
  "interface-font": { name: "interface_font", fits: () => true },
  "terminal-font": { name: "terminal_font", fits: (font) => font.mono },
};

let keep = () => {}; // Hands an appearance to the core.
let keepKeys = async () => {}; // Hands the moved shortcuts to the core.
let recording = null; // The shortcut waiting for its new key.
let listFonts = async () => []; // Asks the core for the installed fonts.
let fonts = null; // The installed fonts, once asked for.
let menu = null; // The font menu open now: `{ id, items, at }`.
let page = "appearance"; // The page shown, kept while the window stays open.

/** Changes part of the appearance, shows it and keeps it. */
function choose(change) {
  const appearance = { ...look.wanted, ...change };
  apply(appearance);
  keep(appearance);
}

// Font menus, as Claude's settings have them: the current choice on a button,
// and a list of every option drawn in its own face, searchable, with a check
// by the one chosen.

async function openMenu(id) {
  fonts ??= await listFonts().catch(() => []);
  const { name, fits } = MENUS[id];
  const chosen = look.wanted[name];
  const items = [{ family: null }, ...fonts.filter(fits)];
  menu = { id, items, at: Math.max(0, items.findIndex((item) => item.family === chosen)) };
  const box = $(id).getBoundingClientRect();
  // Below the button, as tall as the window leaves room for.
  const room = Math.max(180, Math.min(340, innerHeight - box.bottom - 16));
  Object.assign($("font-menu").style, {
    left: `${box.left}px`,
    top: `${box.bottom + 4}px`,
    width: `${box.width}px`,
    maxHeight: `${room}px`,
  });
  $("font-menu").hidden = false;
  $("font-search").value = "";
  drawMenu();
  $("font-search").focus();
}

function closeMenu() {
  if (!menu) return;
  $(menu.id).focus();
  menu = null;
  $("font-menu").hidden = true;
}

/** The menu's items that match the search. */
const shown = () => {
  const query = $("font-search").value.trim().toLowerCase();
  return menu.items.filter((item) => !query || (item.family ?? "theme default").toLowerCase().includes(query));
};

function drawMenu() {
  const chosen = look.wanted[MENUS[menu.id].name];
  const items = shown();
  menu.at = Math.min(menu.at, Math.max(0, items.length - 1));
  $("font-list").replaceChildren(
    ...items.map((item, at) => {
      const row = document.createElement("li");
      row.role = "option";
      row.className = at === menu.at ? "active" : "";
      row.ariaSelected = String(item.family === chosen);
      const label = Object.assign(document.createElement("span"), { textContent: item.family ?? "Theme default" });
      if (item.family) label.style.fontFamily = `"${item.family}"`;
      row.append(label);
      if (item.icons) row.append(Object.assign(document.createElement("small"), { textContent: "Icons" }));
      row.addEventListener("pointerdown", (event) => event.preventDefault());
      row.addEventListener("click", () => pick(item));
      return row;
    }),
  );
  $("font-list").children[menu.at]?.scrollIntoView({ block: "nearest" });
}

function pick(item) {
  choose({ [MENUS[menu.id].name]: item.family });
  closeMenu();
}

function menuKeys(event) {
  const items = shown();
  if (event.key === "ArrowDown" || event.key === "ArrowUp") {
    menu.at = Math.max(0, Math.min(items.length - 1, menu.at + (event.key === "ArrowDown" ? 1 : -1)));
    drawMenu();
  } else if (event.key === "Enter") {
    if (items[menu.at]) pick(items[menu.at]);
  } else if (event.key === "Escape") {
    closeMenu();
  } else {
    return;
  }
  event.preventDefault();
  event.stopPropagation();
}

/** A small picture of a theme: its map, a plate and its accent. */
function card(theme) {
  const button = document.createElement("button");
  button.type = "button";
  button.className = "theme-card";
  button.dataset.theme = theme.id;
  const picture = document.createElement("span");
  picture.className = "swatch";
  picture.innerHTML = "<i></i><b></b><b></b><b></b><u></u>";
  const name = Object.assign(document.createElement("span"), { className: "name", textContent: theme.name });
  const modes = theme.modes.length > 1 ? "Light and dark" : `${theme.modes[0] === "dark" ? "Dark" : "Light"} only`;
  const note = Object.assign(document.createElement("span"), { className: "note", textContent: modes });
  button.append(picture, name, note);
  button.title = theme.description;
  button.addEventListener("click", () => choose({ theme: theme.id }));
  return button;
}

/** Shows the sheet as the look now stands. */
function fill() {
  const { appearance, theme, terminal } = look;
  if (!appearance || $("settings").hidden) return;
  if (!$("theme-choice").children.length) $("theme-choice").append(...[...themes.values()].map(card));
  for (const button of $("theme-choice").children) {
    const shown = themes.get(button.dataset.theme);
    const [canvas, plate, node, accent] = shown.swatch[modeOf(shown, appearance.mode)];
    button.firstChild.style.cssText = `--a:${canvas};--b:${plate};--c:${node};--d:${accent}`;
    button.classList.toggle("on", shown === theme);
  }
  // A theme with one mode shows that mode as the one in use. The choice is
  // still kept, for the next theme that has both.
  const fixed = theme.modes.length === 1;
  const mode = fixed ? theme.modes[0] : appearance.mode;
  for (const button of $("mode-choice").children) button.classList.toggle("on", button.dataset.mode === mode);
  $("mode-choice").classList.toggle("fixed", fixed);
  $("mode-note").textContent = theme.modes.length > 1 ? "" : `${theme.name} has only a ${theme.modes[0]} mode.`;
  for (const [id, { name }] of Object.entries(MENUS)) {
    $(id).textContent = appearance[name] ?? "Theme default";
    $(id).style.fontFamily = appearance[name] ? `"${appearance[name]}"` : "";
  }
  for (const [id, value] of [
    ["terminal-size", appearance.terminal_size],
    ["line-height", appearance.line_height],
  ]) {
    if (document.activeElement !== $(id)) $(id).value = value;
  }
  const preview = $("font-preview");
  Object.assign(preview.style, {
    fontFamily: terminal.fontFamily,
    fontSize: `${terminal.fontSize}px`,
    lineHeight: terminal.lineHeight,
    background: terminal.theme.background,
    color: terminal.theme.foreground,
  });
  for (const span of preview.querySelectorAll("[data-ansi]")) span.style.color = terminal.theme[span.dataset.ansi];
}

// Shortcuts: each on a key that a click lets the person move. The next key
// pressed with Command or Control takes its place, and Escape leaves it.

/** The moved shortcuts, with `id` on `key`. A shortcut put back on its own key is no longer moved. */
function movedWith(id, key) {
  const moved = {};
  for (const action of ACTIONS) {
    const now = action.id === id ? key : keyOf(action.id);
    if (now !== action.key) moved[action.id] = now;
  }
  return moved;
}

async function moveKey(id, key) {
  const moved = movedWith(id, key);
  setMoved(moved);
  drawKeys();
  await keepKeys(moved);
}

function drawKeys(note = "") {
  const groups = [...new Set(ACTIONS.map((action) => action.group))];
  $("key-list").replaceChildren(
    ...groups.flatMap((group) => {
      const head = Object.assign(document.createElement("h3"), { textContent: group });
      const list = document.createElement("dl");
      list.className = "keys";
      for (const action of ACTIONS.filter((item) => item.group === group)) {
        const key = keyOf(action.id);
        const cap = Object.assign(document.createElement("button"), { type: "button", className: "keycap" });
        cap.textContent = recording === action.id ? "Press a key…" : display(key);
        cap.classList.toggle("recording", recording === action.id);
        cap.title = "Click, then press the new key";
        cap.addEventListener("click", () => {
          recording = recording === action.id ? null : action.id;
          drawKeys();
        });
        const row = document.createElement("div");
        const value = document.createElement("dd");
        if (key !== action.key) {
          const back = Object.assign(document.createElement("button"), { type: "button", className: "reset", textContent: "Reset" });
          back.title = `Put back on ${display(action.key)}`;
          back.addEventListener("click", () => moveKey(action.id, action.key));
          value.append(back);
        }
        value.append(cap);
        row.append(Object.assign(document.createElement("dt"), { textContent: action.label }), value);
        list.append(row);
      }
      return [head, list];
    }),
  );
  $("key-note").textContent = note;
}

function recordKey(event) {
  if (!recording) return;
  event.preventDefault();
  event.stopImmediatePropagation();
  if (event.key === "Escape") {
    recording = null;
    return drawKeys();
  }
  const key = pressed(event);
  if (!key) return; // A modifier alone, or a key without Command or Control.
  const taken = ACTIONS.find((action) => action.id !== recording && keyOf(action.id) === key);
  if (taken) return drawKeys(`${display(key)} is already ${taken.label}.`);
  const id = recording;
  recording = null;
  moveKey(id, key);
}

/** Shows one page of the settings. */
function showPage(name) {
  page = name;
  closeMenu();
  recording = null;
  if (name === "shortcuts") drawKeys();
  for (const button of $("settings-nav").querySelectorAll("button")) {
    const on = button.dataset.page === name;
    button.classList.toggle("on", on);
    if (on) $("settings-title").textContent = button.textContent.trim();
  }
  for (const section of $("settings").querySelectorAll("section[data-page]")) section.hidden = section.dataset.page !== name;
}

/** Opens the settings at `name`, or at the page last shown. */
export function openSettings(name = page) {
  $("settings").hidden = false;
  showPage(name);
  fill();
  $("settings-nav").querySelector("button.on")?.focus();
}

export function closeSettings() {
  closeMenu();
  recording = null;
  $("settings").hidden = true;
}

/** Wires the settings up: `appearance` and `shortcuts` hand a change to the core, and `fonts` lists the installed fonts. */
export function setupSettings({ appearance, shortcuts, fonts: installed }) {
  [keep, keepKeys, listFonts] = [appearance, shortcuts, installed];
  // Before every other key handler, so the key being recorded runs nothing.
  window.addEventListener("keydown", recordKey, true);
  watch(fill);
  for (const button of $("settings-nav").querySelectorAll("button")) {
    button.addEventListener("click", () => showPage(button.dataset.page));
  }
  // The desktop app knows its version. A browser on `serve` leaves it out.
  window.__TAURI__?.app
    ?.getVersion()
    .then((version) => ($("version").textContent = `Version ${version}`))
    .catch(() => {});
  $("close-settings").addEventListener("click", closeSettings);
  $("settings").addEventListener("pointerdown", (event) => {
    if (menu && !event.target.closest("#font-menu, .font-choice")) closeMenu();
    if (event.target === $("settings")) closeSettings();
  });
  for (const id of Object.keys(MENUS)) {
    $(id).addEventListener("click", () => (menu?.id === id ? closeMenu() : openMenu(id)));
  }
  $("font-search").addEventListener("input", () => {
    menu.at = 0;
    drawMenu();
  });
  $("font-menu").addEventListener("keydown", menuKeys);
  // Scrolling the sheet would leave the menu behind its button.
  $("settings-sheet").addEventListener("scroll", (event) => event.target.closest?.("#font-menu") || closeMenu(), true);
  $("settings-sheet").addEventListener("submit", (event) => event.preventDefault());
  for (const button of $("mode-choice").children) {
    button.addEventListener("click", () => choose({ mode: button.dataset.mode }));
  }
  for (const [id, name, [least, most]] of [
    ["terminal-size", "terminal_size", [9, 24]],
    ["line-height", "line_height", [1, 2]],
  ]) {
    $(id).addEventListener("change", () => {
      const value = Number($(id).value);
      if (Number.isFinite(value) && value >= least && value <= most) choose({ [name]: value });
      else $(id).value = look.appearance[name];
    });
  }
}
