// Drives the interface from inside the real desktop window and reports what
// happened. `scripts/selftest.sh` starts the app with fixtures in place of the
// store, the sessions and Claude Code, and only then is this file loaded.

import { screenText, setWidth, showList, sky, state } from "./app.js";

const $ = (id) => document.getElementById(id);
const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
const invoke = (command, args) => window.__TAURI__.core.invoke(command, args);

async function until(what, test, ms = 10000) {
  for (let waited = 0; waited < ms; waited += 50) {
    const found = test();
    if (found) return found;
    await sleep(50);
  }
  throw new Error(`timed out waiting for ${what}`);
}

function expect(holds, otherwise) {
  if (!holds) throw new Error(otherwise);
}

// Reading the page. The map is one canvas, so its domains are read from what
// the page was told to draw, and one step checks that they are in fact painted.

const domain = (name) => state.snapshot.domains.find((item) => item.name === name);
const count = (name) => domain(name)?.sessions.length ?? 0;
const links = () => state.snapshot.links.length;
const toast = () => ($("toast").hidden ? "" : $("toast").textContent);
const terminal = () => screenText(0);
const onScreen = () => document.querySelectorAll(".pane:not([hidden])").length;
const titles = (list) => [...$(list).querySelectorAll(".title")].map((node) => node.textContent);
const row = (list, title) =>
  [...$(list).children].find((node) => node.querySelector(".title")?.textContent === title);
const snapshot = () => invoke("map", { request: { command: "snapshot" } });

/** Middle of a domain on the map, or of an element on the page. */
function centre(thing) {
  if (!(thing instanceof Element)) return sky.toScreen(thing.x, thing.y);
  const box = thing.getBoundingClientRect();
  return [box.x + box.width / 2, box.y + box.height / 2];
}

// Acting on the page as a pointer and keyboard would

function pointer(target, type, [x, y], more = {}) {
  const event = { bubbles: true, clientX: x, clientY: y, button: 0, pointerId: 1, ...more };
  target.dispatchEvent(new PointerEvent(type, event));
}

async function clickDomain(name) {
  pointer($("sky"), "pointerdown", centre(domain(name)));
  pointer($("sky"), "pointerup", centre(domain(name)));
  await until(`the panel for ${name}`, () => $("domain-name").value === name);
}

/** Presses on one domain, moves to another and lets go. */
async function drag(from, to, more) {
  const [a, b] = [centre(from), centre(to)];
  pointer($("sky"), "pointerdown", a, more);
  pointer($("sky"), "pointermove", [(a[0] + b[0]) / 2, (a[1] + b[1]) / 2], more);
  pointer($("sky"), "pointermove", b, more);
  pointer($("sky"), "pointerup", b, more);
  await sleep(300);
}

/** Double-clicks the map at a fraction `x` across and `y` down the window. */
function doubleClickMap(x, y) {
  const at = { bubbles: true, clientX: innerWidth * x, clientY: innerHeight * y };
  $("sky").dispatchEvent(new MouseEvent("dblclick", at));
}

async function enterName(text) {
  await until("the name box", () => !$("namer").hidden);
  $("namer-input").value = text;
  $("namer").requestSubmit();
}

async function giveName(text) {
  await enterName(text);
  await until(`the domain ${text}`, () => domain(text));
}

async function addRootDomain(text, x, y) {
  doubleClickMap(x, y);
  await giveName(text);
}

function key(name, more) {
  const event = { key: name, bubbles: true, cancelable: true, ...more };
  (document.activeElement ?? document.body).dispatchEvent(new KeyboardEvent("keydown", event));
}

// The steps, in the order a person would meet them

const steps = [
  ["the agent's history stays out of the session list", async () => {
    await until("three sessions", () => state.snapshot.sessions.length === 3);
    expect(!titles("list-body").length, `the list showed ${titles("list-body")}`);
  }],
  ["double-clicking empty space adds a root domain", async () => {
    await addRootDomain("System", 0.4, 0.3);
    await addRootDomain("AI", 0.6, 0.3);
    await addRootDomain("Memory", 0.5, 0.7);
    expect(state.snapshot.domains.length === 3, "expected three domains");
  }],
  ["every domain is painted as a point of light", async () => {
    await new Promise(requestAnimationFrame);
    await new Promise(requestAnimationFrame);
    const scale = $("sky").width / $("sky").clientWidth;
    for (const item of state.snapshot.domains) {
      const [x, y] = centre(item).map((at) => Math.round(at * scale));
      const [red, green, blue] = $("sky").getContext("2d").getImageData(x, y, 1, 1).data;
      expect(red + green + blue > 400, `${item.name} is dark at its middle`);
    }
  }],
  ["shift-dragging links domains, and a domain can have two parents", async () => {
    await drag(domain("System"), domain("Memory"), { shiftKey: true });
    await drag(domain("AI"), domain("Memory"), { shiftKey: true });
    expect(links() === 2, `expected 2 links, found ${links()}`);
  }],
  ["a link that would form a loop is refused", async () => {
    await drag(domain("Memory"), domain("System"), { shiftKey: true });
    expect(toast().includes("loop"), `the message was "${toast()}"`);
    expect(links() === 2, `expected 2 links, found ${links()}`);
  }],
  ["dragging a domain moves it and the move is saved", async () => {
    const saved = async () => (await snapshot()).domains.find((item) => item.name === "AI");
    const before = await saved();
    const [x, y] = centre(domain("AI"));
    pointer($("sky"), "pointerdown", [x, y]);
    pointer($("sky"), "pointermove", [x + 30, y + 20]);
    pointer($("sky"), "pointermove", [x + 60, y + 40]);
    pointer($("sky"), "pointerup", [x + 60, y + 40]);
    await sleep(300);
    const [nowX, nowY] = centre(domain("AI"));
    const moved = [Math.round(nowX - x), Math.round(nowY - y)];
    expect(moved[0] === 60 && moved[1] === 40, `it moved by ${moved} on screen`);
    const after = await saved();
    expect(after.x > before.x && after.y > before.y, "the new position was not saved");
  }],
  ["sessions join a domain from the history, and its parents count them once", async () => {
    await clickDomain("Memory");
    $("add-existing").click();
    await until("the history", () => titles("picker-list").length === 3);
    row("picker-list", "Alpha").click();
    row("picker-list", "Beta").click();
    $("picker").querySelector("form").requestSubmit();
    await until("the sessions in Memory", () => count("Memory") === 2 && $("picker").hidden);
    expect(count("System") === 2 && count("AI") === 2, "the parents did not count them");
    expect(titles("recent-list").sort().join() === "Alpha,Beta", `the list showed ${titles("recent-list")}`);
  }],
  ["the panel shows a domain's sessions and its parents", async () => {
    expect(titles("domain-sessions").sort().join() === "Alpha,Beta", `sessions were ${titles("domain-sessions")}`);
    pointer($("panel-edge"), "pointerdown", [innerWidth - 320, 300]);
    pointer($("panel-edge"), "pointermove", [innerWidth - 400, 300]);
    pointer($("panel-edge"), "pointerup", [innerWidth - 400, 300]);
    expect($("panel").getBoundingClientRect().width === 400, "the panel kept its width");
    setWidth("panel");
    const parents = [...$("domain-links").querySelectorAll(".go")].map((node) => node.textContent);
    expect(parents.sort().join() === "AI,System", `parents were ${parents}`);
  }],
  ["a subdomain is added from the panel", async () => {
    $("add-subdomain").click();
    await giveName("CXL");
    expect(links() === 3, `expected 3 links, found ${links()}`);
  }],
  ["opening a session fills the window with its terminal, and typing reaches the agent", async (hold) => {
    row("recent-list", "Alpha").click();
    await until("the agent's first line", () => terminal().includes("fake agent --resume"));
    const box = $("shell").getBoundingClientRect();
    expect(box.top === 0 && box.bottom === innerHeight && box.right === innerWidth, "the terminal left a margin");
    const input = document.querySelector(".pane .xterm-helper-textarea");
    input.focus();
    const typed = { data: "hello\r", inputType: "insertText", bubbles: true };
    input.dispatchEvent(new InputEvent("input", typed));
    await until("the agent's answer", () => terminal().includes("got hello"));
    expect(titles("open-list").join() === "Alpha", `open sessions were ${titles("open-list")}`);
    if (hold) await sleep(15000);
  }],
  ["keys switch between the map and the terminal, and hide the list", async () => {
    key("1", { metaKey: true });
    expect($("shell").hidden, "the terminal stayed up");
    key("2", { metaKey: true });
    expect(!$("shell").hidden, "the terminal did not come back");
    key("b", { metaKey: true });
    expect($("shell").getBoundingClientRect().left === 44, "the list stayed");
    key("b", { metaKey: true });
    expect($("shell").getBoundingClientRect().left === 292, "the list did not come back");
  }],
  ["a second session opens beside the first, and hiding one keeps it running", async () => {
    row("recent-list", "Beta").querySelector('[title="Open beside"]').click();
    await until("two terminals", () => onScreen() === 2);
    await until("both agents", () => screenText(1).includes("fake agent --resume"));

    // The edge between them moves the split, and the list's edge its width.
    const [first, box] = [document.querySelector(".pane.first"), $("shell").getBoundingClientRect()];
    pointer($("divider"), "pointerdown", [box.left + box.width / 2, 300]);
    pointer($("divider"), "pointermove", [box.left + box.width * 0.3, 300]);
    pointer($("divider"), "pointerup", [box.left + box.width * 0.3, 300]);
    const share = first.getBoundingClientRect().width / box.width;
    expect(Math.abs(share - 0.3) < 0.02, `the first pane took ${share.toFixed(2)} of the width`);
    pointer($("list-edge"), "pointerdown", [292, 300]);
    pointer($("list-edge"), "pointermove", [344, 300]);
    pointer($("list-edge"), "pointerup", [344, 300]);
    expect($("shell").getBoundingClientRect().left === 344, "the list kept its width");
    setWidth("list");

    document.querySelector('.pane:not([hidden]) [title^="Hide"]').click();
    expect(onScreen() === 1, `${onScreen()} terminals were on screen`);
    expect(titles("open-list").length === 2, `open sessions were ${titles("open-list")}`);
  }],
  ["closing the last session returns to the map", async () => {
    for (const title of ["Alpha", "Beta"]) row("open-list", title).querySelector(".close").click();
    await until("the map", () => $("shell").hidden && !document.querySelector(".pane"));
    expect(!$("panel").hidden, "the domain panel did not come back");
  }],
  ["a shell opens below a session, and its pane closes when the shell exits", async () => {
    row("recent-list", "Alpha").click();
    await until("the agent", () => terminal().includes("fake agent --resume"));
    document.querySelector('.pane [title^="Split down"]').click();
    await until("two terminals", () => onScreen() === 2 && titles("open-list").includes("Shell"));
    const [above, below] = [...document.querySelectorAll(".pane:not([hidden])")].map((node) => node.getBoundingClientRect());
    expect(below.top >= above.bottom && below.left === above.left, "the shell did not open below");
    const input = [...document.querySelectorAll(".pane .xterm-helper-textarea")][1];
    const type = (data) => input.dispatchEvent(new InputEvent("input", { data, inputType: "insertText", bubbles: true }));
    const shell = () => screenText(1);
    await until("the shell's prompt", () => shell().includes("$"));
    type("echo deck-$((6*7))\r");
    await until("the shell's answer", () => shell().includes("deck-42"));
    await sleep(2000);
    type("exit\r");
    await until("the shell's pane to close", () => onScreen() === 1 && titles("open-list").join() === "Alpha");
    row("open-list", "Alpha").querySelector(".close").click();
    await until("the map", () => $("shell").hidden);
  }],
  ["a new session joins its domain at once", async () => {
    $("start-here").click();
    await until("the folder box", () => !$("starter").hidden && $("starter-folder").value);
    const chosen = $("starter-domain").selectedOptions[0].textContent;
    expect(chosen === "Memory", `the domain offered was ${chosen}`);
    $("starter").requestSubmit();
    await until("the agent's first line", () => terminal().includes("fake agent --session-id"));
    await until("the session in Memory", () => count("Memory") === 3);
    expect(titles("domain-sessions").includes("New session"), `sessions were ${titles("domain-sessions")}`);
  }],
  ["a new session that never got a prompt leaves its domain", async () => {
    document.querySelector(".pane .close").click();
    await until("the session to leave", () => count("Memory") === 2 && titles("domain-sessions").length === 2);
    expect((await snapshot()).missing.length === 0, "it was left as a missing session");
  }],
  ["typing a folder in the name box imports its tree of domains", async () => {
    // The fixture sessions ran in the folder that also holds the fixture tree.
    const work = row("domain-sessions", "Beta").querySelector(".folder").textContent;
    doubleClickMap(0.5, 0.9);
    await enterName(`${work}/topics`);
    await until("the imported domains", () => domain("topics") && domain("kernel"));
    const { domains, links: linked } = await snapshot();
    const id = (name) => domains.find((item) => item.name === name)?.id;
    const holds = (parent, child) => linked.some(([a, b]) => a === id(parent) && b === id(child));
    expect(holds("topics", "os") && holds("os", "kernel") && holds("os", "linux"), "the tree lost its shape");
    expect(holds("topics", "memory") && !id(".hidden"), "a folder was missed or a hidden one added");
  }],
];

export async function run(hold) {
  // A made-up pointer cannot be captured, and a glide would move the map
  // while the next step aims at it.
  Element.prototype.setPointerCapture = () => {};
  sky.still = true;

  // The window opens on someone's desk. Their own pointer, wheel and keys are
  // kept out of the page, so a stray move cannot disturb a step.
  for (const type of ["pointerdown", "pointermove", "pointerup", "dblclick", "wheel", "keydown"]) {
    window.addEventListener(type, (event) => event.isTrusted && event.stopImmediatePropagation(), true);
  }

  // The test moves the map and hides the list, which must not replace what the
  // user left. It starts from the list showing, whatever they left.
  Storage.prototype.setItem = () => {};
  showList(true);
  setWidth("list");
  setWidth("panel");

  const results = [];
  // An error nobody caught fails the run, whichever step it happened under.
  const uncaught = [];
  window.addEventListener("error", (event) => uncaught.push(event.message));
  window.addEventListener("unhandledrejection", (event) => uncaught.push(String(event.reason)));
  for (const [name, step] of steps) {
    try {
      await step(hold);
      results.push(`ok    ${name}`);
    } catch (error) {
      // A refusal shows as a message on the page, which usually says why a step failed.
      const said = toast() ? `, and the page said "${toast()}"` : "";
      results.push(`FAIL  ${name}: ${error.message}${said}`);
    }
  }
  results.push(...uncaught.map((message) => `FAIL  uncaught error: ${message}`));
  const failed = results.filter((line) => line.startsWith("FAIL")).length;
  results.push(failed ? `${failed} of ${steps.length} steps failed` : `all ${steps.length} steps passed`);
  await invoke("selftest_done", { report: results.join("\n"), failed: failed > 0 });
}
