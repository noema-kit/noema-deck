// The pages of Settings drawn from what the agents report, laid out as
// Claude's and Codex's desktop apps lay out theirs:
//
// - Usage › Overview: how much of each agent's plan is used, as the agent says.
// - Usage › Analytics: the tokens used, by day, agent, model and session.
// - Profile: lifetime figures and a year of token activity.
//
// The core adds every reply up by day, model and session; this file picks a
// span and an agent, and draws the sums.

const $ = (id) => document.getElementById(id);
const SVG = "http://www.w3.org/2000/svg";
/** The agents in the order their bars stack, bottom first. Each keeps its colour whatever is shown. */
const ORDER = ["claude", "codex", "antigravity"];
/** The spans to choose from. A year is drawn a week to a bar. */
const SPANS = [
  [7, "7d"],
  [30, "30d"],
  [90, "90d"],
  [365, "1y"],
];
/** The agents that answer `/usage` when asked. Codex reports its limits as it works. */
const ASKED = ["claude", "antigravity"];
const ROWS = [5, 15]; // Rows of the top list, folded and unfolded.
const DAY = 86400000;
/** What the pages draw before the first figures arrive: nothing used, shown as waiting. */
const NOTHING = { days: [], sessions: [] };

let lent; // What `app.js` lends: requests, the agents and their marks, and the sessions it lists.
let report = null; // The tokens, as the core last added them up.
let drawn = ""; // The figures on the pages, as text.
let reading = null; // The token request on its way, so two are never sent at once.
let plans = null; // Each agent's plan limits, as last reported.
const asking = new Set(); // The agents being asked for their limits now.
const chosen = { tab: "overview", days: 30, agent: null, top: "sessions", more: false };

/** Takes what the pages need from `app.js`, and fills their controls. */
export function setupUsage(given) {
  lent = given;
  const days = Number(kept("days"));
  if (SPANS.some(([span]) => span === days)) chosen.days = days;
  for (const tab of $("usage-tabs").children) tab.addEventListener("click", () => showTab(tab.dataset.tab));
  segmented($("usage-span"), SPANS, () => chosen.days, (days) => {
    chosen.days = days;
    keep("days", days);
  });
  segmented($("usage-top-kind"), [["sessions", "Sessions"], ["models", "Models"]], () => chosen.top, (top) => {
    chosen.top = top;
  });
  const agents = [["", "All agents"], ...Object.entries(lent.agents)];
  $("usage-agent").replaceChildren(...agents.map(([agent, name]) => el("option", { value: agent }, name)));
  $("usage-agent").addEventListener("change", () => {
    chosen.agent = $("usage-agent").value || null;
    draw();
  });
  $("usage-more").addEventListener("click", () => {
    chosen.more = !chosen.more;
    draw();
  });
  $("limits-again").addEventListener("click", () => checkLimits());
  const redraw = new ResizeObserver(() => draw());
  for (const id of ["usage-chart", "usage-activity"]) redraw.observe($(id));
  showTab(chosen.tab);
}

/** Brings the page Settings is showing up to date: `usage` or `profile`. It
 * shows at once what is known, and fills in the rest as it comes. */
export function showPage(page) {
  draw();
  refreshUsage();
  if (page === "usage" && chosen.tab === "overview") checkLimits();
}

function showTab(tab) {
  chosen.tab = tab;
  for (const button of $("usage-tabs").children) {
    const on = button.dataset.tab === tab;
    button.classList.toggle("on", on);
    button.setAttribute("aria-selected", String(on));
  }
  for (const panel of $("usage-tabs").parentElement.querySelectorAll("[role=tabpanel]")) panel.hidden = panel.dataset.tab !== tab;
  if (tab === "overview" && !$("usage-tabs").closest("[hidden]")) checkLimits();
  draw();
}

/** Reads the records again, which takes a moment only the first time, and draws what changed. */
function refreshUsage() {
  reading ??= lent
    .usage()
    .then(
      (answer) => {
        $("usage-note").textContent = "";
        const text = JSON.stringify(answer);
        if (text === drawn) return;
        [report, drawn] = [answer, text];
        draw();
      },
      (error) => ($("usage-note").textContent = `Could not read the records: ${error.message}`),
    )
    .finally(() => (reading = null));
  return reading;
}

/**
 * Draws the limits known now, then asks Claude Code and Antigravity side by
 * side, each card taking its agent's answer as it comes. An agent already
 * being asked is not asked again.
 */
async function checkLimits() {
  try {
    plans = await lent.limits({ ask: false });
  } catch {
    // The agents' answers still come below.
  }
  drawPlans();
  await Promise.all(ASKED.filter((agent) => !asking.has(agent)).map(async (agent) => {
    asking.add(agent);
    drawPlans();
    try {
      const [plan] = await lent.limits({ ask: true, agent });
      plans = (plans ?? []).map((known) => (known.agent === agent ? plan : known));
    } catch {
      // What was known stays on the page.
    } finally {
      asking.delete(agent);
      drawPlans();
    }
  }));
}

/** Fills a segmented control with `options`, `[value, label]`, the current one on. */
function segmented(box, options, current, pick) {
  const fill = () => {
    for (const button of box.children) {
      const on = button.dataset.value === String(current());
      button.classList.toggle("on", on);
      button.setAttribute("aria-pressed", String(on));
    }
  };
  box.replaceChildren(...options.map(([value, label]) => {
    const option = el("button", { type: "button" }, label);
    option.dataset.value = String(value);
    option.addEventListener("click", () => {
      pick(value);
      fill();
      draw();
    });
    return option;
  }));
  fill();
}

// Sums

const total = (tokens) => tokens.input + tokens.cached + tokens.written + tokens.output;
const empty = () => ({ input: 0, cached: 0, written: 0, output: 0, replies: 0 });

function add(sum, tokens) {
  for (const name of ["input", "cached", "written", "output", "replies"]) sum[name] += tokens[name];
  return sum;
}

/** The local date of `date`, as the core writes days: `2026-11-28`. */
const dayOf = (date) =>
  `${date.getFullYear()}-${String(date.getMonth() + 1).padStart(2, "0")}-${String(date.getDate()).padStart(2, "0")}`;

/** The last `count` days, today last. */
function lastDays(count) {
  const date = new Date();
  date.setHours(12, 0, 0, 0);
  date.setDate(date.getDate() - count + 1);
  return Array.from({ length: count }, () => {
    const day = dayOf(date);
    date.setDate(date.getDate() + 1);
    return day;
  });
}

const wanted = (row, first) => row.day >= first && (!chosen.agent || row.agent === chosen.agent);
const shown = (id) => !$(id).closest("[hidden]");

// Drawing

function draw() {
  const waiting = !report;
  for (const box of document.querySelectorAll(".stats, #usage-chart, #usage-activity")) box.classList.toggle("waiting", waiting);
  if (shown("usage-chart")) {
    const days = lastDays(chosen.days);
    const rows = (report ?? NOTHING).days.filter((row) => wanted(row, days[0]));
    const sum = rows.reduce((sum, row) => add(sum, row), empty());
    stats(sum);
    chart(days, rows, total(sum));
    top(days[0], rows, total(sum));
  }
  if (shown("usage-activity")) {
    profile();
    activity();
  }
}

/** Each agent's plan, as a card of bars in the way Claude's settings show theirs. */
function drawPlans() {
  $("limits-again").disabled = asking.size > 0;
  $("limits-again").textContent = asking.size ? "Checking…" : "Check again";
  if (!plans) return;
  $("usage-plans").replaceChildren(...plans.map((plan) => {
    const asked = asking.has(plan.agent);
    const when = asked ? "Checking…" : plan.checked ? `Updated ${since(plan.checked)}` : "";
    const rows = plan.limits.length
      ? plan.limits.map(limitRow)
      : [el("p", { className: "note" }, asked ? "Asking…" : "No limits reported yet")];
    return el(
      "article",
      { className: "plan" },
      el("header", {}, lent.agentMark(plan.agent), el("h4", {}, lent.agents[plan.agent]), el("span", { className: "note" }, when)),
      ...rows,
    );
  }));
}

function limitRow(limit) {
  const meter = el("div", { className: "meter" }, el("span"));
  meter.firstChild.style.width = `${Math.min(100, Math.max(0, limit.used))}%`;
  meter.classList.toggle("high", limit.used >= 90);
  meter.setAttribute("role", "meter");
  meter.setAttribute("aria-valuenow", String(limit.used));
  return el(
    "div",
    { className: "limit" },
    el("div", {}, el("span", {}, limit.name), el("p", { className: "note" }, limit.resets ? `Resets ${until(limit.resets)}` : "Not started")),
    meter,
    el("span", { className: "used" }, `${Math.round(limit.used)}% used`),
  );
}

/** One strip of figures, as Codex's profile shows them, or a dash for each until they arrive. */
function strip(id, figures) {
  $(id).replaceChildren(...figures.map(([value, label]) => el("div", {}, el("dd", {}, report ? value : "–"), el("dt", {}, label))));
}

function stats(sum) {
  const read = sum.input + sum.cached + sum.written;
  strip("usage-stats", [
    [short(total(sum)), "Tokens"],
    [sum.replies.toLocaleString(), "Replies"],
    [read ? `${((sum.cached / read) * 100).toFixed(1)}%` : "–", "From cache"],
    [short(sum.output), "Output"],
  ]);
}

/** Tokens by day, or by week across a year, stacked by agent, with each agent's share below. */
function chart(days, rows, all) {
  const box = $("usage-chart");
  const week = days.length > 90 ? 7 : 1;
  // The buckets end today; the first may hold fewer days.
  const buckets = [];
  for (let end = days.length; end > 0; end -= week) buckets.unshift(days.slice(Math.max(0, end - week), end));
  const start = new Map(buckets.flatMap((bucket, at) => bucket.map((day) => [day, at])));
  const sums = buckets.map(() => Object.fromEntries(ORDER.map((agent) => [agent, 0])));
  const shares = Object.fromEntries(ORDER.map((agent) => [agent, 0]));
  for (const row of rows) {
    const at = start.get(row.day);
    if (at === undefined) continue;
    sums[at][row.agent] += total(row);
    shares[row.agent] += total(row);
  }
  const agents = ORDER.filter((agent) => (chosen.agent ? agent === chosen.agent : shares[agent] > 0 || !all));
  $("usage-legend").replaceChildren(...(chosen.agent ? [] : agents).map((agent) =>
    el("span", {}, el("i", { className: agent }), lent.agents[agent], el("b", {}, percent(all ? shares[agent] / all : 0)))));

  const width = box.clientWidth || 500;
  const height = 180;
  const [left, right, bottom, top] = [40, 18, 22, 8];
  const most = Math.max(0, ...sums.map((sum) => agents.reduce((every, agent) => every + sum[agent], 0)));
  // Three or so lines across, at round numbers. With nothing used there is only the baseline.
  const every = nice(most / 3);
  const ceiling = every * Math.max(1, Math.ceil(most / every));
  const scale = (value) => ((height - bottom - top) * value) / ceiling;
  const band = (width - left - right) / buckets.length;
  const bar = Math.max(2, Math.min(22, band * 0.62));

  const svg = node("svg", { viewBox: `0 0 ${width} ${height}`, width, height, role: "img" });
  svg.setAttribute("aria-label", `Tokens by ${week > 1 ? "week" : "day"}`);
  for (let value = 0; value <= (most ? ceiling : 0); value += every) {
    const y = height - bottom - scale(value);
    svg.append(node("line", { x1: left, x2: width - right, y1: y, y2: y, class: value ? "grid" : "base" }));
    svg.append(node("text", { x: left - 8, y: y + 4, class: "tick", "text-anchor": "end" }, short(value)));
  }
  const labelled = Math.max(1, Math.ceil(buckets.length / Math.max(2, Math.floor((width - left - right) / 64))));
  buckets.forEach((bucket, at) => {
    const x = left + band * at + (band - bar) / 2;
    const column = node("g", { class: "column" });
    column.append(node("rect", { x: left + band * at, y: top, width: band, height: height - bottom - top, class: "hit" }));
    let base = height - bottom;
    const used = agents.filter((agent) => sums[at][agent] > 0);
    used.forEach((agent, index) => {
      const tall = scale(sums[at][agent]);
      const last = index === used.length - 1;
      // A two-pixel gap of the surface parts each segment from the next.
      const drawn = Math.max(1, last ? tall : tall - 2);
      const round = last ? Math.min(3, bar / 2, drawn) : 0;
      column.append(node("path", { d: segment(x, base - tall + (last ? 0 : 2), bar, drawn, round), class: `fill ${agent}` }));
      base -= tall;
    });
    if (at % labelled === (buckets.length - 1) % labelled) {
      svg.append(node("text", { x: left + band * at + band / 2, y: height - 5, class: "tick", "text-anchor": "middle" }, dateLabel(bucket[0])));
    }
    hover(column, () => [
      week > 1 ? `${dateLabel(bucket[0])} – ${dateLabel(bucket.at(-1))}` : dateLabel(bucket[0], true),
      ...used.map((agent) => [agent, sums[at][agent]]),
    ]);
    svg.append(column);
  });
  box.replaceChildren(svg);
}

/** A bar's segment, its top corners rounded by `round`. */
function segment(x, y, width, height, round) {
  const r = Math.max(0, round);
  return `M${x} ${y + height}V${y + r}Q${x} ${y} ${x + r} ${y}H${x + width - r}Q${x + width} ${y} ${x + width} ${y + r}V${y + height}Z`;
}

/** The sessions, or the models, that used the most tokens. A session the agents list opens on a click. */
function top(first, rows, all) {
  const models = chosen.top === "models";
  const sums = new Map();
  const sources = models ? rows : (report ?? NOTHING).sessions.filter((row) => wanted(row, first));
  for (const row of sources) {
    const key = models ? `${row.agent}\n${row.model}` : `${row.agent}/${row.id}`;
    if (!sums.has(key)) sums.set(key, { key, agent: row.agent, id: row.id, model: row.model || "Unknown", last: row.day, ...empty() });
    const sum = add(sums.get(key), row);
    if (row.day > sum.last) sum.last = row.day;
  }
  const sorted = [...sums.values()].sort((a, b) => total(b) - total(a));
  const listed = sorted.slice(0, ROWS[chosen.more ? 1 : 0]);
  $("usage-more").hidden = sorted.length <= ROWS[0];
  $("usage-more").textContent = chosen.more ? "Show less" : "Show more";
  const [name, detail] = models ? ["Model", "Replies"] : ["Session", "Last used"];
  $("usage-top").tHead.replaceChildren(el(
    "tr",
    {},
    el("th", {}, name),
    el("th", { className: "number" }, detail),
    el("th", { className: "number" }, "Tokens"),
    el("th", { className: "number" }, "Share"),
  ));
  if (!listed.length) {
    const none = report ? "None in this span" : "Reading the agents' records…";
    $("usage-top").tBodies[0].replaceChildren(el("tr", { className: "none" }, el("td", { colSpan: 4 }, none)));
    return;
  }
  $("usage-top").tBodies[0].replaceChildren(...listed.map((sum) => {
    const session = models ? null : lent.session(sum.key);
    const title = models ? sum.model : session?.title ?? `Unlisted ${sum.id.slice(0, 8)}`;
    const hint = models || session ? title : "A run the agent does not offer to resume, such as a sub-agent's";
    const label = el("span", { className: "named" }, lent.agentMark(sum.agent), el("span", { title: hint }, title));
    const row = el(
      "tr",
      {},
      el("td", {}, session ? el("button", { type: "button", title: "Open the session" }, label) : label),
      el("td", { className: "number fine" }, models ? sum.replies.toLocaleString() : dateLabel(sum.last)),
      el("td", { className: "number" }, short(total(sum))),
      el("td", { className: "number fine" }, percent(total(sum) / all)),
    );
    row.classList.toggle("unlisted", !models && !session);
    row.querySelector("button")?.addEventListener("click", () => lent.open(session));
    return row;
  }));
}

/** Tokens used on each day anything was, by every agent. */
function byDay() {
  const sums = new Map();
  for (const row of (report ?? NOTHING).days) sums.set(row.day, (sums.get(row.day) ?? 0) + total(row));
  return sums;
}

/** The Profile page's figures: everything the agents' records hold, and streaks of days in use. */
function profile() {
  const sums = byDay();
  const days = [...sums.keys()].filter((day) => sums.get(day) > 0).sort();
  let [longest, run, before] = [0, 0, null];
  for (const day of days) {
    run = before && next(before) === day ? run + 1 : 1;
    longest = Math.max(longest, run);
    before = day;
  }
  // A streak still runs if today or yesterday was used.
  const [today, yesterday] = lastDays(2).reverse();
  let current = 0;
  for (let day = sums.get(today) ? today : yesterday; sums.get(day); day = previous(day)) current += 1;
  const count = (number, word) => `${number} ${word}${number === 1 ? "" : "s"}`;
  strip("profile-stats", [
    [short([...sums.values()].reduce((all, value) => all + value, 0)), "Lifetime tokens"],
    [short(Math.max(0, ...sums.values())), "Peak day"],
    [count(days.length, "day"), "Days active"],
    [count(longest, "day"), "Longest streak"],
    [count(current, "day"), "Current streak"],
  ]);
}

const shifted = (day, by) => {
  const date = new Date(`${day}T12:00:00`);
  date.setDate(date.getDate() + by);
  return dayOf(date);
};
const next = (day) => shifted(day, 1);
const previous = (day) => shifted(day, -1);

/** A year of days, a column to a week, shaded by the tokens used. */
function activity() {
  const box = $("usage-activity");
  const sums = byDay();
  // Weeks start on Monday; the grid ends with the week holding today.
  const today = new Date();
  today.setHours(12, 0, 0, 0);
  const weeks = 53;
  const first = new Date(today.getTime() - (((today.getDay() + 6) % 7) + (weeks - 1) * 7) * DAY);
  first.setHours(12, 0, 0, 0);
  const used = [...sums.values()].filter((value) => value > 0).sort((a, b) => a - b);
  // Four shades, split at the quartiles of the days anything was used.
  const cuts = [0.25, 0.5, 0.75].map((share) => used[Math.floor(share * (used.length - 1))] ?? 0);
  const shade = (value) => (value <= 0 ? 0 : 1 + cuts.filter((cut) => value > cut).length);

  const width = box.clientWidth || 500;
  const left = 26;
  const step = Math.max(7, (width - left) / weeks);
  const cell = step * 0.78;
  const height = 16 + step * 7;
  const svg = node("svg", { viewBox: `0 0 ${left + step * weeks} ${height}`, width: left + step * weeks, height, role: "img" });
  svg.setAttribute("aria-label", "Tokens by day across the past year");
  ["Mon", "Wed", "Fri"].forEach((name, at) => svg.append(node("text", { x: 0, y: 16 + step * (at * 2) + cell - 1, class: "tick" }, name)));
  let [month, labelled] = [-1, -3];
  for (let at = 0; at < weeks * 7; at += 1) {
    const date = new Date(first.getTime() + at * DAY);
    if (date > today) break;
    const [column, row] = [Math.floor(at / 7), at % 7];
    // A month is named over its first week, unless the grid starts partway
    // through it or the name would crowd the one before.
    if (row === 0 && date.getMonth() !== month) {
      month = date.getMonth();
      if ((column > 0 || date.getDate() <= 7) && column - labelled >= 3 && column < weeks - 2) {
        svg.append(node("text", { x: left + column * step, y: 9, class: "tick" }, date.toLocaleDateString(undefined, { month: "short" })));
        labelled = column;
      }
    }
    const day = dayOf(date);
    const value = sums.get(day) ?? 0;
    const square = node("rect", { x: left + column * step, y: 16 + row * step, width: cell, height: cell, rx: cell * 0.3, class: `day shade-${shade(value)}` });
    hover(square, () => [dateLabel(day, true), ["", value]]);
    svg.append(square);
  }
  box.replaceChildren(svg);
}

// The hover note

/** Shows a note by the pointer over `target`: a heading, then each `[agent, tokens]`. */
function hover(target, lines) {
  target.addEventListener("pointerenter", (event) => {
    const [heading, ...rows] = lines();
    $("usage-tip").replaceChildren(el("b", {}, heading), ...rows.map(([agent, value]) =>
      el("p", {}, agent ? el("i", { className: agent }) : "", agent ? `${lent.agents[agent]} ` : "", el("span", {}, `${short(value)} tokens`))));
    $("usage-tip").hidden = false;
    place(event);
  });
  target.addEventListener("pointermove", place);
  target.addEventListener("pointerleave", () => ($("usage-tip").hidden = true));
}

function place(event) {
  const note = $("usage-tip");
  note.style.left = `${Math.min(event.clientX + 14, innerWidth - note.offsetWidth - 8)}px`;
  note.style.top = `${Math.min(event.clientY + 14, innerHeight - note.offsetHeight - 8)}px`;
}

// Words and numbers

/** A count in at most four figures, such as 12.3M. */
function short(count) {
  for (const [unit, name] of [[1e9, "B"], [1e6, "M"], [1e3, "K"]]) {
    if (count >= unit) return `${(count / unit).toFixed(count >= unit * 100 ? 0 : 1)}${name}`;
  }
  return String(Math.round(count));
}

const percent = (share) => (share >= 0.001 || !share ? `${((share || 0) * 100).toFixed(1)}%` : "<0.1%");

/** The least of 1, 2, 2.5 or 5 times a power of ten that is at least `value`. */
function nice(value) {
  if (value <= 0) return 1;
  const power = 10 ** Math.floor(Math.log10(value));
  return [1, 2, 2.5, 5, 10].map((step) => step * power).find((step) => step >= value);
}

function dateLabel(day, long = false) {
  const date = new Date(`${day}T12:00:00`);
  const options = long ? { weekday: "short", month: "short", day: "numeric" } : { month: "short", day: "numeric" };
  return date.toLocaleDateString(undefined, options);
}

/** When a moment in Unix seconds comes, such as "in 3 days". */
function until(seconds) {
  const hours = (seconds * 1000 - Date.now()) / 3600000;
  if (hours < 1) return "within the hour";
  if (hours < 48) return `in ${Math.round(hours)} hours`;
  return `in ${Math.round(hours / 24)} days`;
}

/** How long ago a moment in Unix seconds was, such as "2 hours ago". */
function since(seconds) {
  const minutes = (Date.now() - seconds * 1000) / 60000;
  if (minutes < 2) return "just now";
  if (minutes < 120) return `${Math.round(minutes)} minutes ago`;
  if (minutes < 48 * 60) return `${Math.round(minutes / 60)} hours ago`;
  return `${Math.round(minutes / 1440)} days ago`;
}

// Elements

function el(tag, props = {}, ...children) {
  const made = Object.assign(document.createElement(tag), props);
  made.append(...children.filter((child) => child !== "" && child !== null && child !== undefined));
  return made;
}

function node(tag, attributes = {}, text) {
  const made = document.createElementNS(SVG, tag);
  for (const [name, value] of Object.entries(attributes)) made.setAttribute(name, value);
  if (text !== undefined) made.textContent = text;
  return made;
}

function keep(name, value) {
  try {
    localStorage.setItem(`noema-deck-usage-${name}`, value);
  } catch {
    // The choice holds for this run only.
  }
}

function kept(name) {
  try {
    return localStorage.getItem(`noema-deck-usage-${name}`);
  } catch {
    return null;
  }
}
