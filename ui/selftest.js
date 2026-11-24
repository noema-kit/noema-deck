// Drives the interface from inside the real desktop window and reports what
// happened. `scripts/selftest.sh` starts the app with fixtures in place of the
// store, the sessions and Claude Code, and only then is this file loaded.

import { sky, state } from "./app.js";

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

// Reading the page. The map is one canvas, so its stars are read from what the
// page was told to draw, and one step checks that they are in fact painted.

const star = (name) => state.snapshot.stars.find((item) => item.name === name);
const count = (name) => star(name)?.sessions.length ?? 0;
const links = () => state.snapshot.links.length;
const toast = () => ($("toast").hidden ? "" : $("toast").textContent);
const terminal = () => document.querySelector(".pane .xterm-rows")?.textContent ?? "";
const titles = (list) => [...$(list).querySelectorAll(".session .title")].map((node) => node.textContent);
const row = (list, title) =>
  [...$(list).querySelectorAll(".session")].find((node) => node.querySelector(".title").textContent === title);
const snapshot = () => invoke("map", { request: { command: "snapshot" } });

/** Middle of a star on the map, or of an element on the page. */
function centre(thing) {
  if (!(thing instanceof Element)) return sky.toScreen(thing.x, thing.y);
  const box = thing.getBoundingClientRect();
  return [box.x + box.width / 2, box.y + box.height / 2];
}

/** What a press on a star or an element lands on. */
const under = (thing) => (thing instanceof Element ? thing : $("sky"));

// Acting on the page as a pointer and keyboard would

function pointer(target, type, [x, y], more = {}) {
  const event = { bubbles: true, clientX: x, clientY: y, button: 0, pointerId: 1, ...more };
  target.dispatchEvent(new PointerEvent(type, event));
}

async function clickStar(name) {
  pointer($("sky"), "pointerdown", centre(star(name)));
  pointer($("sky"), "pointerup", centre(star(name)));
  await until(`the panel for ${name}`, () => $("star-name").value === name);
}

/** Presses on `from`, moves to `to` and lets go, as one drag over the sky or from a list. */
async function drag(from, to, over, more) {
  const [a, b] = [centre(from), centre(to)];
  pointer(under(from), "pointerdown", a, more);
  pointer(over, "pointermove", [(a[0] + b[0]) / 2, (a[1] + b[1]) / 2], more);
  pointer(over, "pointermove", b, more);
  pointer(over, "pointerup", b, more);
  await sleep(300);
}

/** Double-clicks the sky at a fraction `x` across and `y` down the window. */
function doubleClickSky(x, y) {
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
  await until(`the star ${text}`, () => star(text));
}

async function addTopStar(text, x, y) {
  doubleClickSky(x, y);
  await giveName(text);
}

function key(name, more) {
  const event = { key: name, bubbles: true, cancelable: true, ...more };
  (document.activeElement ?? document.body).dispatchEvent(new KeyboardEvent("keydown", event));
}

// The steps, in the order a person would meet them

const steps = [
  ["the tray lists the sessions", async () => {
    await until("three sessions", () => titles("tray-list").length === 3);
    expect(titles("tray-list").includes("Alpha"), `titles were ${titles("tray-list")}`);
  }],
  ["double-clicking empty space adds a top star", async () => {
    await addTopStar("System", 0.45, 0.3);
    await addTopStar("AI", 0.75, 0.3);
    await addTopStar("Memory", 0.6, 0.7);
    expect(state.snapshot.stars.length === 3, "expected three stars");
  }],
  ["every star is painted as a point of light", async () => {
    await new Promise(requestAnimationFrame);
    await new Promise(requestAnimationFrame);
    const scale = $("sky").width / $("sky").clientWidth;
    for (const item of state.snapshot.stars) {
      const [x, y] = centre(item).map((at) => Math.round(at * scale));
      const [red, green, blue] = $("sky").getContext("2d").getImageData(x, y, 1, 1).data;
      expect(red + green + blue > 400, `${item.name} is dark at its middle`);
    }
  }],
  ["shift-dragging links stars, and a star can have two larger stars", async () => {
    await drag(star("System"), star("Memory"), $("sky"), { shiftKey: true });
    await drag(star("AI"), star("Memory"), $("sky"), { shiftKey: true });
    expect(links() === 2, `expected 2 links, found ${links()}`);
  }],
  ["a link that would form a loop is refused", async () => {
    await drag(star("Memory"), star("System"), $("sky"), { shiftKey: true });
    expect(toast().includes("loop"), `the message was "${toast()}"`);
    expect(links() === 2, `expected 2 links, found ${links()}`);
  }],
  ["dragging a session onto a star places it, and larger stars count it once", async () => {
    await drag(row("tray-list", "Alpha"), star("Memory"), document);
    await until("the session on Memory", () => count("Memory") === 1);
    expect(count("System") === 1 && count("AI") === 1, "larger stars did not count it");
    expect(titles("tray-list").length === 2, "the session stayed in the tray");
  }],
  ["dragging a star moves it and the move is saved", async () => {
    const saved = async () => (await snapshot()).stars.find((item) => item.name === "AI");
    const before = await saved();
    const [x, y] = centre(star("AI"));
    pointer($("sky"), "pointerdown", [x, y]);
    pointer($("sky"), "pointermove", [x + 30, y + 20]);
    pointer($("sky"), "pointermove", [x + 60, y + 40]);
    pointer($("sky"), "pointerup", [x + 60, y + 40]);
    await sleep(300);
    const [nowX, nowY] = centre(star("AI"));
    const moved = [Math.round(nowX - x), Math.round(nowY - y)];
    expect(moved[0] === 60 && moved[1] === 40, `it moved by ${moved} on screen`);
    const after = await saved();
    expect(after.x > before.x && after.y > before.y, "the new position was not saved");
  }],
  ["the panel shows a star's sessions and its larger stars", async () => {
    await clickStar("Memory");
    expect(titles("star-sessions").join() === "Alpha", `sessions were ${titles("star-sessions")}`);
    const larger = [...$("star-links").querySelectorAll(".go")].map((node) => node.textContent);
    expect(larger.sort().join() === "AI,System", `larger stars were ${larger}`);
  }],
  ["a smaller star is added from the panel", async () => {
    $("add-smaller").click();
    await giveName("CXL");
    expect(links() === 3, `expected 3 links, found ${links()}`);
  }],
  ["opening a session shows the agent in a pane, and typing reaches it", async (hold) => {
    row("star-sessions", "Alpha").querySelector(".open").click();
    await until("the agent's first line", () => terminal().includes("fake agent --resume"));
    const input = document.querySelector(".pane .xterm-helper-textarea");
    input.focus();
    const typed = { data: "hello\r", inputType: "insertText", bubbles: true };
    input.dispatchEvent(new InputEvent("input", typed));
    await until("the agent's answer", () => terminal().includes("got hello"));
    if (hold) await sleep(15000);
  }],
  ["one key switches between the map and the pane", async () => {
    key("1", { metaKey: true });
    expect($("shell").hidden, "the shell view stayed up");
    key("2", { metaKey: true });
    expect(!$("shell").hidden, "the shell view did not come back");
  }],
  ["closing the pane returns to the map", async () => {
    document.querySelector(".pane .close").click();
    await until("the map", () => $("shell").hidden && !document.querySelector(".pane"));
  }],
  ["a new session sits on its star at once", async () => {
    $("new-session").click();
    await until("the folder box", () => !$("starter").hidden && $("starter-folder").value);
    $("starter").requestSubmit();
    await until("the agent's first line", () => terminal().includes("fake agent --session-id"));
    await until("the session on Memory", () => count("Memory") === 2);
    expect(titles("star-sessions").includes("New session"), `sessions were ${titles("star-sessions")}`);
  }],
  ["a new session that never got a prompt leaves its star", async () => {
    document.querySelector(".pane .close").click();
    await until("the session to leave", () => count("Memory") === 1 && titles("star-sessions").length === 1);
    expect((await snapshot()).missing.length === 0, "it was left as a missing session");
  }],
  ["typing a folder in the name box imports its tree of stars", async () => {
    // The fixture sessions ran in the folder that also holds the fixture tree.
    const work = row("tray-list", "Beta").querySelector(".folder").textContent;
    doubleClickSky(0.5, 0.9);
    await enterName(`${work}/topics`);
    await until("the imported stars", () => star("topics") && star("kernel"));
    const { stars, links: linked } = await snapshot();
    const id = (name) => stars.find((item) => item.name === name)?.id;
    const holds = (larger, smaller) => linked.some(([a, b]) => a === id(larger) && b === id(smaller));
    expect(holds("topics", "os") && holds("os", "kernel") && holds("os", "linux"), "the tree lost its shape");
    expect(holds("topics", "memory") && !id(".hidden"), "a folder was missed or a hidden one added");
  }],
];

export async function run(hold) {
  // A made-up pointer cannot be captured, and a glide would move the stars
  // while the next step aims at them.
  Element.prototype.setPointerCapture = () => {};
  sky.still = true;

  // The window opens on someone's desk. Their own pointer, wheel and keys are
  // kept out of the page, so a stray move cannot disturb a step.
  for (const type of ["pointerdown", "pointermove", "pointerup", "dblclick", "wheel", "keydown"]) {
    window.addEventListener(type, (event) => event.isTrusted && event.stopImmediatePropagation(), true);
  }

  // The test pans and zooms the map, which must not replace the view the user left.
  const store = Storage.prototype.setItem;
  Storage.prototype.setItem = function (name, value) {
    if (name !== "noema-deck-view") store.call(this, name, value);
  };

  const results = [];
  for (const [name, step] of steps) {
    try {
      await step(hold);
      results.push(`ok    ${name}`);
    } catch (error) {
      results.push(`FAIL  ${name}: ${error.message}`);
    }
  }
  const failed = results.filter((line) => line.startsWith("FAIL")).length;
  results.push(failed ? `${failed} of ${steps.length} steps failed` : `all ${steps.length} steps passed`);
  await invoke("selftest_done", { report: results.join("\n"), failed: failed > 0 });
}
