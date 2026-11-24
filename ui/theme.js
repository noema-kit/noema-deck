// Themes, and the person's choice of theme, mode and typefaces. A theme is a
// folder under `themes/` with a manifest, a style sheet of tokens and a map
// painter (see `docs/themes.md`). The core keeps the choice; this file puts it
// on the page and tells whoever watches, such as the map and the terminals.

const KEPT = "noema-deck-look"; // What `boot.js` reads to open in the last run's look.
const dark = matchMedia("(prefers-color-scheme: dark)");
const root = document.documentElement;

/** Every theme by its folder name, with its manifest. */
export const themes = new Map();

/** What is on the page now, and the appearance last asked for, which may still be on its way. */
export const look = { appearance: null, wanted: null, theme: null, mode: null, painter: null, terminal: null };

const watchers = [];
let turn = 0; // The latest change asked for, so an older one that finishes later is dropped.

/** Calls `watcher` with the look each time it changes. */
export const watch = (watcher) => watchers.push(watcher);

/** Reads the manifest of every theme the app ships. */
export async function loadThemes() {
  const names = await fetch("themes/themes.json").then((response) => response.json());
  for (const name of names) {
    const manifest = await fetch(`themes/${name}/theme.json`).then((response) => response.json());
    themes.set(name, { id: name, ...manifest });
  }
}

/** The mode a theme is shown in: the one asked for, or the theme's own when it has only one. */
export function modeOf(theme, mode) {
  const wanted = mode === "system" ? (dark.matches ? "dark" : "light") : mode;
  return theme.modes.includes(wanted) ? wanted : theme.modes[0];
}

/** A typeface's name as a style sheet takes it: quoted when it has spaces and no quotes or commas of its own. */
const family = (name) => (/\s/.test(name) && !/[",']/.test(name) ? `"${name}"` : name);

/** Puts an appearance on the page. */
export async function apply(appearance) {
  const mine = (turn += 1);
  look.wanted = appearance;
  const theme = themes.get(appearance.theme) ?? themes.get("noema");
  const mode = modeOf(theme, appearance.mode);
  const sheet = document.getElementById("theme");
  const href = `themes/${theme.id}/theme.css`;
  if (sheet.getAttribute("href") !== href) {
    await new Promise((loaded) => {
      sheet.onload = sheet.onerror = loaded;
      sheet.setAttribute("href", href);
    });
  }
  const painter = (await import(`./themes/${theme.id}/map.js`)).default;
  if (mine !== turn) return;

  const fonts = {
    "--font-ui": appearance.interface_font && `${family(appearance.interface_font)}, system-ui, sans-serif`,
    "--font-mono": appearance.terminal_font && `${family(appearance.terminal_font)}, monospace`,
  };
  for (const [name, value] of Object.entries(fonts)) {
    if (value) root.style.setProperty(name, value);
    else root.style.removeProperty(name);
  }
  Object.assign(root.dataset, { theme: theme.id, mode });

  const mono = getComputedStyle(root).getPropertyValue("--font-mono").trim();
  await document.fonts.load(`${appearance.terminal_size}px ${mono}`).catch(() => {});
  if (mine !== turn) return;
  const terminal = {
    theme: theme.terminal[mode],
    fontFamily: mono,
    fontSize: appearance.terminal_size,
    lineHeight: appearance.line_height,
  };
  Object.assign(look, { appearance, theme, mode, painter, terminal });
  try {
    localStorage.setItem(KEPT, JSON.stringify({ theme: theme.id, mode, fonts }));
  } catch {
    // The next run opens in the default look until the core answers.
  }
  for (const watcher of watchers) watcher(look);
}

// Following the system between light and dark.
dark.addEventListener("change", () => {
  if (look.appearance?.mode === "system") apply(look.appearance);
});
