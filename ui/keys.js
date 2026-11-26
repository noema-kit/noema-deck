// Deck's keyboard shortcuts and the keys they are on. Each has its own key,
// and the person can move it to another in Settings; the core keeps the ones
// moved. A key is written as the modifiers held, then the key's code:
// `mod+shift+KeyD`, where `mod` is Command on macOS and Control elsewhere.

const mac = navigator.platform.startsWith("Mac") || navigator.userAgent.includes("Mac");

/** Every shortcut, in the order Settings lists them, with its own key. */
export const ACTIONS = [
  { id: "map", label: "Map", group: "Window", key: "mod+Digit1" },
  { id: "shell", label: "Terminal", group: "Window", key: "mod+Digit2" },
  { id: "list", label: "Session list", group: "Window", key: "mod+KeyB" },
  { id: "search", label: "Search sessions", group: "Window", key: "mod+KeyK" },
  { id: "settings", label: "Settings", group: "Window", key: "mod+Comma" },
  { id: "split_right", label: "Split the pane right", group: "Terminal", key: "mod+KeyD" },
  { id: "split_down", label: "Split the pane down", group: "Terminal", key: "mod+shift+KeyD" },
  { id: "fit", label: "Show every domain", group: "Map", key: "mod+Digit0" },
];

let moved = {}; // The keys the person chose, by action.

/** The key an action is on now. */
export const keyOf = (id) => moved[id] ?? ACTIONS.find((action) => action.id === id).key;

/** Takes the shortcuts the core keeps. */
export function setMoved(shortcuts) {
  moved = { ...shortcuts };
  label();
}

const NAMES = {
  Comma: ",", Period: ".", Slash: "/", Semicolon: ";", Quote: "'", Backquote: "`",
  BracketLeft: "[", BracketRight: "]", Backslash: "\\", Minus: "-", Equal: "=",
  Enter: "↩", Space: "Space", Tab: "⇥", Backspace: "⌫", Escape: "⎋",
  ArrowLeft: "←", ArrowRight: "→", ArrowUp: "↑", ArrowDown: "↓",
};

/** A key as the keys printed on a keyboard, such as ⇧⌘D, or Ctrl+Shift+D elsewhere than macOS. */
export function display(key) {
  const parts = key.split("+");
  const code = parts.pop();
  const name = NAMES[code] ?? code.replace(/^(Key|Digit|Numpad)/, "");
  const signs = mac
    ? { ctrl: "⌃", alt: "⌥", shift: "⇧", mod: "⌘" }
    : { mod: "Ctrl+", ctrl: "Ctrl+", alt: "Alt+", shift: "Shift+" };
  const order = ["ctrl", "alt", "shift", "mod"].filter((part) => parts.includes(part));
  return order.map((part) => signs[part]).join("") + name;
}

/**
 * The key a press makes, or null without Command or Control, which every
 * shortcut is held with. Elsewhere than macOS, Control with a letter belongs
 * to the terminal, so a letter's shortcut takes Shift as well, and that Shift
 * is not part of the key.
 */
export function pressed(event) {
  if (["Meta", "Control", "Alt", "Shift"].includes(event.key)) return null;
  const letter = event.code.startsWith("Key");
  let shift = event.shiftKey;
  if (!mac && event.ctrlKey && letter) {
    if (!event.shiftKey) return null;
    shift = false;
  }
  const held = mac ? event.metaKey : event.ctrlKey;
  if (!held && !(mac && event.ctrlKey)) return null;
  const parts = [];
  if (mac && event.ctrlKey) parts.push("ctrl");
  if (event.altKey) parts.push("alt");
  if (shift) parts.push("shift");
  if (held) parts.push("mod");
  return [...parts, event.code].join("+");
}

/** The action a key press runs, if any. */
export function actionFor(event) {
  const key = pressed(event);
  if (!key) return null;
  return ACTIONS.find((action) => keyOf(action.id) === key)?.id ?? null;
}

/** Puts each shortcut's key in the titles and placeholders that mention it. */
export function label() {
  for (const node of document.querySelectorAll("[data-action]")) {
    const key = display(keyOf(node.dataset.action));
    const base = node.dataset.label ?? (node.dataset.label = node.getAttribute(node.placeholder !== undefined ? "placeholder" : "title"));
    node.setAttribute(node.placeholder !== undefined ? "placeholder" : "title", `${base} (${key})`);
  }
}
