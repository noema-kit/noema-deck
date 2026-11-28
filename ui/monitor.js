// The Monitor view: what the machine is doing, as its processors, memory,
// graphics, network and disks over the last two minutes, then what each open
// pane's processes add up to, and every process.
//
// The core reads the machine; this file asks while the view shows, keeps the
// last two minutes of readings, and draws them. Each chart glides one reading
// behind, so its line moves steadily and never jumps.

const $ = (id) => document.getElementById(id);
const EVERY = 2000; // Milliseconds between readings.
const SPAN = 120000; // Milliseconds of readings a chart shows.
const FRAME = 50; // Milliseconds between frames. A chart moves a few pixels a second, which needs no more.
const HEAD = 6; // Pixels a chart keeps free at its right edge, for the dot at the head of each line.
const BREAK = EVERY * 3; // Readings further apart than this are not joined: the view was away between them.
const ROWS = [12, 60]; // Rows of the process list, folded and unfolded.
/** The process list's columns, each sorted largest first, or names from A. */
const COLUMNS = [
  ["name", "Process"],
  ["user", "User"],
  ["pid", "PID"],
  ["cpu", "CPU"],
  ["memory", "Memory"],
];
const still = matchMedia("(prefers-reduced-motion: reduce)");

let lent; // What `app.js` lends: the request, and the open panes by ID.
let chosenView = false; // Whether the monitor is the view chosen, though the window may be out of sight.
let showing = false; // Whether it is being read and drawn.
let turn = 0; // Counts the times the view was shown, so a reading asked for before it left is dropped.
let sample = null; // The last reading.
let colors = null; // The page's colours, as the charts draw them.
const charts = {};
const chosen = { sort: "cpu", query: "", more: false };

function el(tag, props = {}, ...children) {
  const node = Object.assign(document.createElement(tag), props);
  node.append(...children.filter((child) => child !== null && child !== undefined && child !== false));
  return node;
}

// Figures

/** A count of bytes in at most four figures. Memory counts in units of 1,024 and disks in units of 1,000, as macOS shows them. */
function bytes(count, base = 1000) {
  let unit = "B";
  for (const next of ["KB", "MB", "GB", "TB"]) {
    if (count < base) break;
    [count, unit] = [count / base, next];
  }
  return unit === "B" ? `${Math.round(count)} B` : `${count < 100 ? count.toFixed(1) : Math.round(count)} ${unit}`;
}

const memory = (count) => bytes(count, 1024);
const rate = (count) => `${bytes(count)}/s`;
const percent = (share) => `${Math.round(share)}%`;
const share = (part, whole) => (whole ? (part / whole) * 100 : 0);

/** How long ago, for a point on a chart. */
const ago = (ms) => (ms < 1500 ? "Now" : `${Math.round(ms / 1000)} s ago`);

function uptime(seconds) {
  const [days, hours, minutes] = [Math.floor(seconds / 86400), Math.floor((seconds % 86400) / 3600), Math.floor((seconds % 3600) / 60)];
  return days ? `${days}d ${hours}h` : hours ? `${hours}h ${minutes}m` : `${minutes}m`;
}

/** The round figure at or above `value`, as 1, 2 or 5 times a power of ten, for the top of a chart. */
function round(value) {
  const power = 10 ** Math.floor(Math.log10(value));
  return power * [1, 2, 5, 10].find((step) => step * power >= value);
}

// Charts

/** A colour written as `#rrggbb`, let through by `alpha`. */
function faded(hex, alpha) {
  const [r, g, b] = [1, 3, 5].map((at) => parseInt(hex.slice(at, at + 2), 16));
  return `rgb(${r} ${g} ${b} / ${alpha})`;
}

/** Reads the colours the charts draw in from the page, which a theme or a mode changes. */
export function restyleMonitor() {
  const style = getComputedStyle(document.documentElement);
  const read = (name) => style.getPropertyValue(name).trim();
  colors = { in: read("--series-in"), out: read("--series-out"), line: read("--line"), muted: read("--muted"), surface: read("--surface") };
}

/**
 * One chart of the last two minutes. `series` names what it draws, each as
 * `{ color, label, show }`. A chart of shares runs to `top`, 100; a chart of
 * rates runs to a round figure above the most it shows, never below `floor`.
 * With two series the second is drawn downwards from a line across the
 * middle, on the same scale as the first.
 */
class Chart {
  constructor(plot, { series, top = null, floor = 1 }) {
    Object.assign(this, { series, fixed: top, floor });
    this.canvas = plot.querySelector("canvas");
    this.ceiling = plot.querySelector(".ceiling");
    this.points = [];
    this.top = top ?? floor; // The top of the scale now, which eases towards the one wanted.
    this.over = null; // Where the pointer is over the chart, from its left edge.
    this.told = null; // The reading the tip describes.
    plot.addEventListener("pointermove", (event) => {
      this.over = event.clientX - plot.getBoundingClientRect().left;
      this.pointer = [event.clientX, event.clientY];
      this.told = null;
      this.draw(performance.now());
    });
    plot.addEventListener("pointerleave", () => {
      this.over = this.told = null;
      $("monitor-tip").hidden = true;
      this.draw(performance.now());
    });
  }

  push(at, values) {
    this.points.push({ at, values });
    while (this.points.length > 1 && this.points[1].at < at - SPAN - BREAK) this.points.shift();
  }

  draw(now) {
    const { canvas, points, series } = this;
    const [width, height] = [canvas.clientWidth, canvas.clientHeight];
    if (!width || !height || !colors) return;
    const ratio = devicePixelRatio || 1;
    if (canvas.width !== Math.round(width * ratio) || canvas.height !== Math.round(height * ratio)) {
      [canvas.width, canvas.height] = [Math.round(width * ratio), Math.round(height * ratio)];
    }
    const pen = canvas.getContext("2d");
    pen.setTransform(ratio, 0, 0, ratio, 0, 0);
    pen.clearRect(0, 0, width, height);

    // The newest reading waits past the right edge and glides in as the next
    // is taken, so the line at the edge is always between two readings. For
    // someone who asked for less motion, a chart ends at its newest reading
    // and stands still until the next.
    const clock = still.matches ? (points.at(-1)?.at ?? now) : now - EVERY;
    const edge = width - HEAD; // Where the lines end, which leaves room for the dot at their head.
    const x = (at) => edge - ((clock - at) / SPAN) * edge;
    const mirrored = series.length > 1;
    const base = mirrored ? Math.round(height / 2) + 0.5 : height - 0.5;
    const room = mirrored ? height / 2 - 3 : height - 4;
    const y = (value, which) => base + (which ? 1 : -1) * (Math.min(value, this.top) / this.top) * room;

    const most = Math.max(this.floor, ...points.filter((point) => x(point.at) > 0).flatMap((point) => point.values));
    const wanted = this.fixed ?? round(most);
    this.top = Math.abs(wanted - this.top) < wanted * 0.01 ? wanted : this.top + (wanted - this.top) * 0.2;
    if (this.ceiling && this.ceiling.dataset.top !== String(wanted)) {
      this.ceiling.dataset.top = wanted;
      this.ceiling.textContent = rate(wanted);
    }

    pen.lineWidth = 1;
    pen.strokeStyle = colors.line;
    pen.beginPath();
    for (const part of mirrored ? [] : [0.25, 0.5, 0.75]) {
      const line = Math.round(height * part) + 0.5;
      pen.moveTo(0, line);
      pen.lineTo(width, line);
    }
    pen.moveTo(0, base);
    pen.lineTo(width, base);
    pen.stroke();

    // Readings taken without a break between them, each as a line of its own.
    const runs = [];
    for (const [at, point] of points.entries()) {
      if (at && point.at - points[at - 1].at <= BREAK) runs.at(-1).push(point);
      else runs.push([point]);
    }
    pen.lineJoin = pen.lineCap = "round";
    pen.save();
    pen.beginPath();
    pen.rect(0, 0, edge, height);
    pen.clip();
    const heads = []; // Where each line meets the edge, if it reaches it.
    series.forEach(({ color }, which) => {
      const wash = pen.createLinearGradient(0, y(this.top, which), 0, base);
      wash.addColorStop(0, faded(colors[color], 0.3));
      wash.addColorStop(1, faded(colors[color], 0.03));
      for (const run of runs) {
        const line = run.map((point) => [x(point.at), y(point.values[which], which)]);
        // A reading that is late leaves the line level until it comes.
        const [endX, endY] = line.at(-1);
        if (endX < edge && run === runs.at(-1) && clock - run.at(-1).at < BREAK) line.push([edge, endY]);
        const past = line.findIndex(([px]) => px >= edge);
        if (past > 0) {
          const [[ax, ay], [bx, by]] = [line[past - 1], line[past]];
          heads.push([color, ay + ((by - ay) * (edge - ax)) / (bx - ax || 1)]);
        }
        pen.beginPath();
        for (const [at, [px, py]] of line.entries()) pen[at ? "lineTo" : "moveTo"](px, py);
        pen.strokeStyle = colors[color];
        pen.lineWidth = 1.75;
        pen.stroke();
        pen.lineTo(line.at(-1)[0], base);
        pen.lineTo(line[0][0], base);
        pen.closePath();
        pen.fillStyle = wash;
        pen.fill();
      }
    });
    pen.restore();
    // The head of each line: a dot where the latest reading is coming in.
    for (const [color, at] of heads) {
      pen.beginPath();
      pen.arc(edge, at, HEAD, 0, Math.PI * 2);
      pen.fillStyle = faded(colors[color], 0.22);
      pen.fill();
      pen.beginPath();
      pen.arc(edge, at, 2.5, 0, Math.PI * 2);
      pen.fillStyle = colors[color];
      pen.fill();
    }
    if (this.over !== null) this.point(pen, clock, x, y, height);
  }

  /** Marks the reading under the pointer and says what it was. */
  point(pen, clock, x, y, height) {
    const at = (point) => Math.abs(x(point.at) - this.over);
    const near = this.points.reduce((best, point) => (!best || at(point) < at(best) ? point : best), null);
    if (!near || at(near) > 24) {
      $("monitor-tip").hidden = true;
      return;
    }
    const px = x(near.at);
    pen.strokeStyle = faded(colors.muted, 0.5);
    pen.lineWidth = 1;
    pen.beginPath();
    pen.moveTo(Math.round(px) + 0.5, 0);
    pen.lineTo(Math.round(px) + 0.5, height);
    pen.stroke();
    this.series.forEach(({ color }, which) => {
      pen.beginPath();
      pen.arc(px, y(near.values[which], which), 4, 0, Math.PI * 2);
      pen.fillStyle = colors[color];
      pen.fill();
      pen.strokeStyle = colors.surface;
      pen.lineWidth = 2;
      pen.stroke();
    });
    const when = ago(clock - near.at);
    if (this.told !== `${near.at} ${when}`) {
      this.told = `${near.at} ${when}`;
      const rows = this.series.map(({ color, label, show }, which) =>
        el("p", {}, el("i", { className: `key ${color}` }), label, el("span", { textContent: show(near.values[which]) })),
      );
      $("monitor-tip").replaceChildren(el("b", { textContent: when }), ...rows);
    }
    const tip = $("monitor-tip");
    tip.hidden = false;
    const [left, top] = this.pointer;
    tip.style.left = `${Math.min(left + 14, innerWidth - tip.offsetWidth - 8)}px`;
    tip.style.top = `${Math.max(8, top - tip.offsetHeight - 12)}px`;
  }
}

/** Draws every chart again, a few times a second while the view shows, unless less motion was asked for. */
function frame(mine, last = 0) {
  if (!showing || mine !== turn || still.matches) return;
  requestAnimationFrame((now) => {
    if (now - last >= FRAME) {
      for (const chart of Object.values(charts)) chart.draw(now);
      last = now;
    }
    frame(mine, last);
  });
}

// Readings

/** Asks for readings one after another, as long as the view shows. */
async function follow(mine) {
  while (showing && mine === turn) {
    const asked = performance.now();
    try {
      const answer = await lent.sample();
      if (!showing || mine !== turn) return;
      $("monitor-note").textContent = "";
      take(answer);
    } catch (error) {
      $("monitor-note").textContent = `Could not read the machine: ${error.message}`;
    }
    await new Promise((resolve) => setTimeout(resolve, Math.max(200, EVERY - (performance.now() - asked))));
  }
}

/** Takes a reading: its figures go on the page and its points on the charts. */
function take(answer) {
  sample = answer;
  const now = performance.now();
  const { cpu, memory: held, gpus, network, disk } = sample;
  charts.cpu.push(now, [cpu.usage]);
  charts.memory.push(now, [share(held.used, held.total)]);
  if (gpus.length) charts.gpu.push(now, [gpus[0].usage]);
  charts.network.push(now, [network.received, network.sent]);
  charts.disk.push(now, [disk.read, disk.written]);
  $("monitor-body").classList.remove("waiting");
  draw();
  // Without the frames that move them, the charts are drawn as each reading comes.
  if (still.matches) for (const chart of Object.values(charts)) chart.draw(now);
}

/** Fills a bar to a share of its track, turning it to the warning colour when nearly full. */
function fill(meter, part) {
  meter.style.width = `${Math.min(100, part)}%`;
  meter.parentElement.classList.toggle("high", part >= 90);
}

// A reading changes figures, seldom what there is to show. So the page keeps
// its rows and writes the new figures into them, and a row is made or dropped
// only as the number of rows changes: the page is not laid out again, the row
// under the pointer stays the same row, and the reader stays where they were.

/** Writes text into a node, unless it says so already. */
function write(node, text) {
  if (node.textContent !== String(text)) node.textContent = text;
}

/** Gives `parent` exactly `count` children, made by `make` as more are needed, and answers with them. */
function sized(parent, count, make) {
  while (parent.children.length > count) parent.lastChild.remove();
  while (parent.children.length < count) parent.append(make());
  return [...parent.children];
}

/** Draws what a reading holds. */
function draw() {
  // The desktop window's WebKit put a reader back at the top when the rows of
  // two tables were made again in one redraw. The rows are kept now, and so
  // is the reader's place, should rows coming and going do the same.
  const body = $("monitor-body");
  const place = body.scrollTop;
  const { host, cpu, memory: held, gpus, network, disk } = sample;
  write($("monitor-name"), host.name.replace(/\.local$/, ""));
  write($("monitor-host"), [host.system, `up ${uptime(host.uptime)}`].filter(Boolean).join(" · "));

  write($("cpu-figure"), percent(cpu.usage));
  write($("cpu-model"), [host.cpu, `${cpu.cores.length} cores`].filter(Boolean).join(" · "));
  write($("cpu-load"), `Load ${cpu.load.map((load) => load.toFixed(2)).join("  ")}`);
  sized($("cpu-cores"), cpu.cores.length, () => el("i", {}, el("b"))).forEach((core, at) => {
    core.title = `Core ${at + 1}: ${percent(cpu.cores[at])}`;
    core.firstChild.style.height = `${Math.max(3, Math.min(100, cpu.cores[at]))}%`;
  });

  write($("memory-figure"), memory(held.used));
  write($("memory-share"), percent(share(held.used, held.total)));
  write($("memory-total"), `${memory(held.total)} in all`);
  write($("swap-figure"), held.swap_total ? `${memory(held.swap_used)} of ${memory(held.swap_total)}` : "None");
  fill($("swap-meter"), share(held.swap_used, held.swap_total));

  // Without a graphics processor that reports, the network and the disk share its row.
  $("gpu-gauge").hidden = !gpus.length;
  $("gauges").classList.toggle("two", !gpus.length);
  if (gpus.length) {
    write($("gpu-figure"), percent(gpus[0].usage));
    write($("gpu-model"), gpus[0].name);
  }

  write($("network-in"), rate(network.received));
  write($("network-out"), rate(network.sent));
  write($("network-total"), `${bytes(network.total_received)} in, ${bytes(network.total_sent)} out since start`);
  write($("disk-in"), rate(disk.read));
  write($("disk-out"), rate(disk.written));
  const volume = () => el("div", { className: "volume" }, el("span"), el("span", { className: "fine" }), el("div", { className: "meter" }, el("span")));
  sized($("disk-volumes"), disk.volumes.length, volume).forEach((row, at) => {
    const { name, mount, total, available } = disk.volumes[at];
    const [title, figures, meter] = row.children;
    row.title = mount;
    write(title, name);
    write(figures, `${bytes(total - available)} of ${bytes(total)}`);
    fill(meter.firstChild, share(total - available, total));
  });

  drawPanes();
  drawProcesses();
  if (body.scrollTop !== place) body.scrollTop = place;
}

/** Shows a pane as a list names it, its agent's mark and then its name, in `node`. */
function nameIn(node, pane) {
  const shows = `${pane.id} ${pane.name}`;
  if (node.shows === shows) return;
  node.shows = shows;
  node.replaceChildren(el("span", { className: "named" }, pane.mark(), el("span", { textContent: pane.name })));
}

/** A row of figures: `first`, then `count` cells of numbers. */
const figuresRow = (first, count) => el("tr", {}, el("td", {}, first), ...Array.from({ length: count }, () => el("td", { className: "number" })));

/** What each open pane's processes add up to, busiest first. A row opens its pane. */
function drawPanes() {
  const loads = sample.panes
    .map((load) => ({ ...load, open: lent.pane(load.pane) }))
    .filter((load) => load.open)
    .sort((a, b) => b.cpu - a.cpu || b.memory - a.memory);
  $("monitor-panes-part").hidden = !loads.length;
  const make = () => {
    const open = el("button", { type: "button", title: "Show this pane" });
    const row = figuresRow(open, 3);
    open.addEventListener("click", () => row.pane.show());
    return row;
  };
  sized($("monitor-panes").tBodies[0], loads.length, make).forEach((row, at) => {
    const load = loads[at];
    const [open, processes, cpu, held] = row.cells;
    row.pane = load.open;
    nameIn(open.firstChild, load.open);
    write(processes, load.processes);
    write(cpu, `${load.cpu.toFixed(1)}%`);
    write(held, memory(load.memory));
  });
}

/**
 * The processes that match the filter, sorted by the chosen column. The
 * filter reads a process's name, owner and number, and the name of the pane
 * it was started in, so a pane's name lists what runs in it.
 */
function drawProcesses() {
  const query = chosen.query.trim().toLowerCase();
  const paneOf = (process) => (process.pane !== null && lent.pane(process.pane)) || null;
  const rows = sample.processes.filter((process) =>
    `${process.name}\n${process.user}\n${process.pid}\n${paneOf(process)?.name ?? ""}`.toLowerCase().includes(query),
  );
  const text = chosen.sort === "name" || chosen.sort === "user";
  rows.sort((a, b) => (text ? a[chosen.sort].localeCompare(b[chosen.sort]) : b[chosen.sort] - a[chosen.sort]) || b.cpu - a.cpu || a.pid - b.pid);

  write($("process-count"), rows.length);
  for (const head of $("monitor-processes").tHead.rows[0].cells) {
    const on = head.dataset.column === chosen.sort;
    head.firstChild.classList.toggle("on", on);
    if (on) head.setAttribute("aria-sort", text ? "ascending" : "descending");
    else head.removeAttribute("aria-sort");
  }

  const listed = rows.slice(0, ROWS[Number(chosen.more)]);
  // The pane a process was started in follows its name, on the rows that have one.
  const make = () => figuresRow(el("span", { className: "named" }, el("span"), el("span", { className: "chip plain", hidden: true })), 4);
  sized($("monitor-processes").tBodies[0], listed.length, make).forEach((row, at) => {
    const process = listed[at];
    const [first, user, pid, cpu, held] = row.cells;
    const [title, chip] = first.firstChild.children;
    const pane = paneOf(process);
    write(title, process.name);
    chip.hidden = !pane;
    if (pane) nameIn(chip, pane);
    write(user, process.user);
    write(pid, process.pid);
    write(cpu, `${process.cpu.toFixed(1)}%`);
    write(held, memory(process.memory));
  });
  $("process-none").hidden = listed.length > 0;
  $("process-more").hidden = rows.length <= ROWS[0];
  write($("process-more"), chosen.more ? "Show fewer" : `Show ${Math.min(rows.length, ROWS[1]) - ROWS[0]} more`);
}

// The view

/** Takes what the view needs from `app.js`, and sets up its charts and controls. */
export function setupMonitor(given) {
  lent = given;
  const one = (color, label, show) => [{ color, label, show }];
  const two = (first, second) => [
    { color: "in", label: first, show: rate },
    { color: "out", label: second, show: rate },
  ];
  charts.cpu = new Chart($("cpu-plot"), { series: one("in", "CPU", percent), top: 100 });
  charts.memory = new Chart($("memory-plot"), { series: one("in", "Memory", percent), top: 100 });
  charts.gpu = new Chart($("gpu-plot"), { series: one("in", "GPU", percent), top: 100 });
  // A quiet network or disk draws against ten kilobytes a second, so a few bytes do not fill the chart.
  charts.network = new Chart($("network-plot"), { series: two("Received", "Sent"), floor: 10000 });
  charts.disk = new Chart($("disk-plot"), { series: two("Read", "Written"), floor: 10000 });

  const heads = ["Pane", "Processes", "CPU", "Memory"];
  $("monitor-panes").tHead.replaceChildren(el("tr", {}, ...heads.map((head, at) => el("th", { className: at ? "number" : "", textContent: head }))));
  $("monitor-processes").tHead.replaceChildren(el("tr", {}, ...COLUMNS.map(([column, label], at) => {
    const sort = el("button", { type: "button", textContent: label });
    sort.addEventListener("click", () => {
      chosen.sort = column;
      if (sample) drawProcesses();
    });
    const head = el("th", { className: at > 1 ? "number" : "" }, sort);
    head.dataset.column = column;
    return head;
  })));

  $("process-search").addEventListener("input", () => {
    chosen.query = $("process-search").value;
    if (sample) drawProcesses();
  });
  $("process-more").addEventListener("click", () => {
    chosen.more = !chosen.more;
    drawProcesses();
  });
  // The charts move again as soon as motion is wanted again.
  still.addEventListener("change", () => frame(turn));
  // Nothing is read or drawn for a window nobody can see.
  document.addEventListener("visibilitychange", () => showMonitor(chosenView));
  restyleMonitor();
}

/** Shows or hides the view. While it shows, the machine is read every two seconds. */
export function showMonitor(shown) {
  chosenView = shown;
  $("monitor").hidden = !shown;
  const visible = shown && !document.hidden;
  if (visible === showing) return;
  showing = visible;
  turn += 1;
  if (!showing) {
    $("monitor-tip").hidden = true;
    return;
  }
  restyleMonitor();
  follow(turn);
  frame(turn);
}
