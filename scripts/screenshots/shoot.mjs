// Takes the README's screenshots: drives a headless Chrome over the DevTools
// protocol through Deck's own interface, served from the made-up world.
// Usage: shoot.mjs DEBUG_PORT PAGE_URL FOLDER
import { writeFileSync } from "node:fs";

const [debug, page, out] = process.argv.slice(2);
const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

const target = await (await fetch(`http://127.0.0.1:${debug}/json/new?${page}`, { method: "PUT" })).json();
const socket = new WebSocket(target.webSocketDebuggerUrl);
await new Promise((resolve) => (socket.onopen = resolve));
let last = 0;
const waiting = new Map();
socket.onmessage = (event) => {
  const message = JSON.parse(event.data);
  waiting.get(message.id)?.(message);
  waiting.delete(message.id);
};
const call = (method, params = {}) =>
  new Promise((resolve) => {
    waiting.set(++last, resolve);
    socket.send(JSON.stringify({ id: last, method, params }));
  });
async function run(source) {
  const { result } = await call("Runtime.evaluate", { expression: `(async () => { ${source} })()`, awaitPromise: true, returnByValue: true });
  if (result.exceptionDetails) throw new Error(JSON.stringify(result.exceptionDetails));
  return result.result.value;
}
async function shot(name) {
  await sleep(700);
  // Clicks made by the script leave a focus ring a person's would not.
  await run(`if (!document.activeElement?.closest('.pane')) document.activeElement?.blur();`);
  const { result } = await call("Page.captureScreenshot", { format: "jpeg", quality: 92 });
  writeFileSync(`${out}/${name}.jpg`, Buffer.from(result.data, "base64"));
  console.log("took", name);
}

await call("Emulation.setDeviceMetricsOverride", { width: 1280, height: 800, deviceScaleFactor: 2, mobile: false });
await call("Page.navigate", { url: page });
await sleep(2500);

// Helpers inside the page.
await run(`
  const app = await import("./app.js");
  const $ = (id) => document.getElementById(id);
  const wait = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
  app.map.still = true;
  // Headless Chrome's software WebGL draws terminals stretched, so they draw
  // as plain elements, as Deck does wherever WebGL is missing.
  window.WebglAddon = undefined;
  window.deck = {
    app, $, wait,
    async look(theme, mode, page) {
      $("show-settings").click(); await wait(300);
      $("theme-choice").querySelector('[data-theme="' + theme + '"]').click(); await wait(500);
      if (mode) { $("mode-choice").querySelector('[data-mode="' + mode + '"]').click(); await wait(500); }
      if (page) { $("settings-nav").querySelector('[data-page="' + page + '"]').click(); await wait(900); }
      else { $("close-settings").click(); await wait(300); }
    },
    row: (name) => [...document.querySelectorAll("#tree .node.domain")].find((node) => node.querySelector(".title")?.textContent === name),
    async unfold(...names) { for (const name of names) { const row = deck.row(name); if (row.getAttribute("aria-expanded") === "false") row.click(); await wait(150); } },
    async fit() { app.map.fit(app.state.listed ? app.state.widths.list : 0, innerWidth - ($("panel").hidden ? 0 : app.state.widths.panel)); await wait(400); },
    async type(pane, text) { await app.send("pane", { command: "write", args: { pane: pane.id, data: text } }); },
  };
`);

for (const mode of ["light", "dark"]) {
  // The map, the tree and a domain's sessions.
  await run(`
    await deck.look("noema", "${mode}");
    deck.app.show("map");
    await deck.unfold("System", "Operating system", "Kernel");
    deck.row("Kernel").querySelector('[title="Show on the map"]').click(); await deck.wait(400);
    await deck.fit();
  `);
  await shot(`map-${mode}`);

  // Usage and the profile, in Settings.
  await run(`
    await deck.look("noema", "${mode}", "usage");
    [...document.querySelectorAll(".tabs button")].find((button) => button.textContent === "Analytics").click();
    await deck.wait(900);
  `);
  await shot(`usage-${mode}`);
  await run(`deck.$("settings-nav").querySelector('[data-page="profile"]').click(); await deck.wait(900);`);
  await shot(`profile-${mode}`);

  // Tiled terminals: an editor, the history and the files.
  await run(`
    deck.$("close-settings").click(); await deck.wait(200);
    for (const pane of [...deck.app.panes]) pane.node.querySelector(".close").click();
    await deck.wait(400);
    // The panes are laid out first, and each is typed in once its prompt shows.
    deck.$("new-shell").click(); await deck.wait(1200);
    deck.app.panes[0].node.querySelector('[title^="Split right"]').click(); await deck.wait(1200);
    deck.app.panes[1].node.querySelector('[title^="Split down"]').click(); await deck.wait(1200);
    const [editor, history, files] = deck.app.panes;
    const ready = (pane) => pane.id !== null && pane.term.buffer.active.getLine(pane.term.buffer.active.cursorY)?.translateToString().includes("❯");
    for (let tries = 0; tries < 100 && ![editor, history, files].every(ready); tries++) await deck.wait(100);
    await deck.type(editor, "cd ~/code/kernel && clear && vim -u NONE -c 'syntax on' -c 'set number tabstop=4 nowrap laststatus=0' mm/fault.c\\r");
    await deck.type(history, "cd ~/code/kernel && clear && git log --oneline --graph -n 9\\r");
    await deck.type(files, "cd ~/code/kernel && clear && wc -l mm/* Makefile\\r");
    await deck.wait(1500);
    editor.term.focus();
  `);
  await shot(`terminal-${mode}`);
  await run(`for (const pane of [...deck.app.panes]) pane.node.querySelector(".close").click(); await deck.wait(400);`);
}

// Galaxy.
await run(`
  await deck.look("galaxy");
  deck.app.show("map");
  deck.$("close-panel").click(); await deck.wait(300);
  await deck.fit();
`);
await shot("galaxy");
socket.close();
