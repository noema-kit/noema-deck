// The Appearance sheet: theme, colour mode and typefaces. Each change shows at
// once and is kept by the core.

import { apply, look, modeOf, themes, watch } from "./theme.js";

const $ = (id) => document.getElementById(id);
// What each font menu offers and where its choice is kept: every installed
// family for the interface, and those whose letters share one width for terminals.
const MENUS = {
  "interface-font": { name: "interface_font", fits: () => true },
  "terminal-font": { name: "terminal_font", fits: (font) => font.mono },
};

let keep = () => {}; // Hands an appearance to the core.
let listFonts = async () => []; // Asks the core for the installed fonts.
let fonts = null; // The installed fonts, once asked for.
let menu = null; // The font menu open now: `{ id, items, at }`.

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
  const room = Math.max(160, Math.min(300, innerHeight - box.bottom - 16));
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
  for (const button of $("mode-choice").children) {
    button.classList.toggle("on", button.dataset.mode === appearance.mode);
  }
  $("mode-choice").classList.toggle("fixed", theme.modes.length === 1);
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

export function openSettings() {
  $("settings").hidden = false;
  fill();
  $("settings").querySelector(".theme-card.on, button")?.focus();
}

export function closeSettings() {
  closeMenu();
  $("settings").hidden = true;
}

/** Wires the sheet up. `save` hands an appearance to the core, and `installed` lists the fonts. */
export function setupSettings(save, installed) {
  [keep, listFonts] = [save, installed];
  watch(fill);
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
