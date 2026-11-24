// The sky: every star, link and name painted on one canvas. Panning or zooming
// repaints a frame and builds no elements, and nothing is painted while the map
// is still. This file knows nothing of sessions or requests: `app.js` hands it
// domains, each drawn as a star, with their links, and asks it where things are.

const TAU = Math.PI * 2;
const SAVED = "noema-deck-view"; // Where the pan and zoom are kept between runs.
const ZOOM = [0.2, 3]; // The smallest and largest scale the map takes.
const REACH = 9; // A star's light dies out this many core radii from its middle.
const TILE = 480; // Side of the repeating tile of far stars.
const GLIDE = 460; // Milliseconds a glide to a new view takes.
const RING = 320; // Milliseconds the focus ring takes to close on its star.

// Blackbody colours from about 3000 K to 20000 K, coolest first. A star worked
// on today burns blue-white, and it cools to amber over three months of quiet.
const HEAT = ["#ffb46b", "#ffd1a3", "#ffe4ce", "#fff3ef", "#e6ebff", "#c9d7ff", "#aabfff"];
const UNLIT = "#b4bfe6"; // A star that holds no sessions yet.

// How long a kind of star's four spikes run, in core radii, and how strong they are.
const SPIKES = { top: [8.8, 1], lit: [6.6, 0.8], unlit: [4.4, 0.45] };

const FONT = {
  name: 'italic 400 13.5px "Newsreader", Georgia, serif',
  top: '500 16px "Newsreader", Georgia, serif',
  count: '600 10.5px "Instrument Sans", system-ui, sans-serif',
};

// Far stars from the farthest layer to the nearest: how far a layer drifts when
// the map pans, how it swells when the map zooms, how many specks one tile
// holds and how large a speck may be.
const DUST = [
  { seed: 11, drift: 0.05, swell: 0.08, count: 110, size: 0.8 },
  { seed: 23, drift: 0.12, swell: 0.16, count: 46, size: 1.1 },
  { seed: 37, drift: 0.24, swell: 0.28, count: 16, size: 1.4 },
];

// What the gas glows with: the red, green and blue one cloud adds at its
// middle, and the share of clouds that take that colour.
const GAS = [
  [[1.5, 3.1, 9], 0.5],
  [[0.7, 4.4, 5.8], 0.2],
  [[4.2, 2, 7.6], 0.2],
  [[6.6, 3.1, 2.8], 0.1],
];
const SPACE = [5, 6, 14]; // The sky where no gas glows.

const clamp = (value, least, most) => Math.min(most, Math.max(least, value));
const mod = (value, by) => ((value % by) + by) % by;
/** A six-digit colour at `alpha`, spelled the way a canvas takes it. */
const fade = (colour, alpha) => colour + Math.round(alpha * 255).toString(16).padStart(2, "0");

/** Random numbers that repeat from a seed, so the backdrop is the same sky on every run. */
function random(seed) {
  return () => {
    seed = (seed + 0x6d2b79f5) | 0;
    let mixed = Math.imul(seed ^ (seed >>> 15), 1 | seed);
    mixed = (mixed + Math.imul(mixed ^ (mixed >>> 7), 61 | mixed)) ^ mixed;
    return ((mixed ^ (mixed >>> 14)) >>> 0) / 4294967296;
  };
}

/** Paints once onto a surface that is never shown, to draw from it every frame after. */
function paint(width, height, draw) {
  const canvas = Object.assign(document.createElement("canvas"), { width, height });
  draw(canvas.getContext("2d"));
  return canvas;
}

/** 1 for a star worked on today, cooling to 0 over three months of quiet. */
function heat(updated) {
  if (!updated) return 0;
  const days = Math.max(0, Date.now() / 1000 - updated) / 86400;
  return Math.max(0, 1 - Math.log1p(days) / Math.log1p(90));
}

/** A star's light: a white core, a bloom in its colour and four thin spikes. */
function starlight(size, tint, [reach, strength]) {
  return paint(size, size, (ctx) => {
    const [half, core] = [size / 2, size / 2 / REACH];
    ctx.translate(half, half);
    ctx.globalCompositeOperation = "lighter";

    // The bloom falls off steeply and then trails a long way, as light does in a lens.
    const bloom = ctx.createRadialGradient(0, 0, 0, 0, 0, half);
    const falloff = [[0, 1], [0.7, 1], [1, 0.86], [1.5, 0.42], [2.5, 0.17], [4, 0.066], [6.5, 0.018], [REACH, 0]];
    for (const [radii, alpha] of falloff) {
      bloom.addColorStop(radii / REACH, fade(radii < 1 ? "#ffffff" : tint, alpha));
    }
    ctx.fillStyle = bloom;
    ctx.fillRect(-half, -half, size, size);

    const [length, width] = [reach * core, core * 0.17];
    const beam = ctx.createLinearGradient(0, 0, length, 0);
    beam.addColorStop(0, fade("#ffffff", strength));
    beam.addColorStop(0.3, fade(tint, strength * 0.5));
    beam.addColorStop(1, fade(tint, 0));
    ctx.fillStyle = beam;
    for (let spike = 0; spike < 4; spike += 1) {
      ctx.beginPath();
      ctx.moveTo(0, -width);
      ctx.quadraticCurveTo(length * 0.25, -width * 0.3, length, 0);
      ctx.quadraticCurveTo(length * 0.25, width * 0.3, 0, width);
      ctx.fill();
      ctx.rotate(TAU / 4);
    }
  });
}

// Each light is painted once at a few sizes and scaled a little from the
// nearest, because a thin spike breaks up when it is shrunk a long way.
const lights = new Map();

function light(kind, tint, size) {
  const name = `${kind} ${tint} ${size}`;
  if (!lights.has(name)) lights.set(name, starlight(size, tint, SPIKES[kind]));
  return lights.get(name);
}

/** One tile of far stars, which repeats across the whole sky. */
function dust({ seed, count, size }, ratio) {
  return paint(TILE * ratio, TILE * ratio, (ctx) => {
    const next = random(seed);
    ctx.scale(ratio, ratio);
    for (let speck = 0; speck < count; speck += 1) {
      // Kept off the edges, so no speck is cut where two tiles meet.
      const [x, y] = [8 + next() * (TILE - 16), 8 + next() * (TILE - 16)];
      const radius = 0.3 + next() ** 3 * size;
      ctx.fillStyle = fade(HEAT[Math.floor(next() * HEAT.length)], 0.22 + 0.6 * next());
      ctx.beginPath();
      ctx.arc(x, y, radius, 0, TAU);
      ctx.fill();
    }
  });
}

/** The glowing gas behind everything: a faint band from the lower left to the upper right. */
function nebula() {
  // The glow is summed as fractions on a coarse grid and only then turned into
  // pixels with a little grain. Painted straight in the 256 levels a canvas
  // keeps, a glow this dark shows as bands.
  const [cols, rows, scale] = [300, 190, 4];
  const gas = new Float32Array(cols * rows * 3);
  const next = random(1977);
  /** A place in the band, which is densest down its middle and widest halfway along. */
  const along = (width = 1) => {
    const far = next();
    const across = (next() + next() + next() - 1.5) * (0.07 + 0.12 * Math.sin(far * Math.PI));
    return [far * 1.3 - 0.15, 1 - far + across * width];
  };
  /** Runs `change` over the grid around a cloud, with how much of the cloud reaches each cell. */
  const cloud = ([u, v], radius, change) => {
    const [cx, cy, reach] = [u * cols, v * rows, radius * 2];
    for (let y = Math.max(0, Math.floor(cy - reach)); y < Math.min(rows, cy + reach); y += 1) {
      for (let x = Math.max(0, Math.floor(cx - reach)); x < Math.min(cols, cx + reach); x += 1) {
        change((y * cols + x) * 3, Math.exp(-((x - cx) ** 2 + (y - cy) ** 2) / radius ** 2));
      }
    }
  };

  // A few wide clouds give the band its body, many small ones its texture, and
  // dark lanes of dust then cut through it.
  for (let puff = 0; puff < 260; puff += 1) {
    const wide = puff < 60;
    let pick = next();
    const [colour] = GAS.find(([, share]) => (pick -= share) < 0) ?? GAS[0];
    const strength = wide ? 0.46 : 0.28 + 0.66 * next();
    cloud(along(), cols * (wide ? 0.06 + 0.1 * next() : 0.012 + 0.035 * next()), (at, fall) => {
      for (let channel = 0; channel < 3; channel += 1) gas[at + channel] += colour[channel] * strength * fall;
    });
  }
  for (let lane = 0; lane < 34; lane += 1) {
    const depth = 0.25 + 0.45 * next();
    cloud(along(0.5), cols * (0.012 + 0.03 * next()), (at, fall) => {
      for (let channel = 0; channel < 3; channel += 1) gas[at + channel] *= 1 - depth * fall;
    });
  }

  return paint(cols * scale, rows * scale, (ctx) => {
    const image = ctx.createImageData(cols * scale, rows * scale);
    for (let y = 0; y < image.height; y += 1) {
      const fy = Math.min(rows - 1.001, y / scale);
      const [row, down] = [Math.floor(fy), fy % 1];
      for (let x = 0; x < image.width; x += 1) {
        const fx = Math.min(cols - 1.001, x / scale);
        const [col, right] = [Math.floor(fx), fx % 1];
        const [above, below] = [(row * cols + col) * 3, ((row + 1) * cols + col) * 3];
        const [at, grain] = [(y * image.width + x) * 4, Math.random() - 0.5];
        for (let channel = 0; channel < 3; channel += 1) {
          const top = gas[above + channel] + (gas[above + 3 + channel] - gas[above + channel]) * right;
          const low = gas[below + channel] + (gas[below + 3 + channel] - gas[below + channel]) * right;
          image.data[at + channel] = SPACE[channel] + top + (low - top) * down + grain;
        }
        image.data[at + 3] = 255;
      }
    }
    ctx.putImageData(image, 0, 0);

    // Stars too far to tell apart, thickest where the gas is.
    for (let speck = 0; speck < 900; speck += 1) {
      const [u, v] = along(1.3);
      ctx.fillStyle = fade("#dfe6ff", 0.05 + 0.2 * next());
      ctx.beginPath();
      ctx.arc(u * image.width, v * image.height, 0.4 + 0.5 * next(), 0, TAU);
      ctx.fill();
    }
  });
}

export class Sky {
  view = { x: 0, y: 0, k: 1 }; // A world point (x, y) is drawn at x * k + view.x.
  fresh = true; // Whether no earlier run left a view behind.
  still = matchMedia("(prefers-reduced-motion: reduce)").matches; // Whether every move is a jump.
  focus = null; // ID of the focused star.
  hover = null; // ID of the star under the pointer.
  drop = null; // ID of the star a drag would land on.
  rubber = null; // A link being drawn: `{ from: star ID, x, y }`, ending at a screen point.

  #canvas;
  #ctx;
  #ink = {};
  #ratio = 0;
  #width = 0;
  #height = 0;
  #bodies = new Map();
  #links = [];
  #gas = nebula();
  #dust = [];
  #frame = 0;
  #last = 0;
  #glide = null;
  #zoom = null;
  #fling = null;
  #pan = null;
  #ringAt = 0;
  #shown = "";
  #saving = 0;

  constructor(canvas) {
    this.#canvas = canvas;
    this.#ctx = canvas.getContext("2d", { alpha: false });
    const style = getComputedStyle(document.documentElement);
    for (const name of ["starlight", "gold", "link"]) {
      this.#ink[name] = style.getPropertyValue(`--${name}`).trim();
    }
    new ResizeObserver(() => this.#resize()).observe(canvas);
    this.#resize();
    this.#restore();
    this.redraw();
    // A canvas does not ask for a typeface, so it is asked for here, and names
    // measured before it arrives are measured again.
    for (const font of Object.values(FONT)) {
      document.fonts.load(font).then(() => {
        for (const body of this.#bodies.values()) body.wide = undefined;
        this.redraw();
      });
    }
  }

  // The scene

  /**
   * Takes the stars and links to draw. A star's place is read each time it is
   * painted, so a star being dragged follows the pointer.
   */
  chart(stars, links) {
    const hanging = new Set(links.map(([, smaller]) => smaller));
    const bodies = stars.map((star) => {
      const [count, top, warmth] = [star.sessions.length, !hanging.has(star.id), heat(star.updated)];
      return {
        star,
        top,
        count,
        kind: !count ? "unlit" : top ? "top" : "lit",
        tint: count ? HEAT[Math.round(warmth * (HEAT.length - 1))] : UNLIT,
        core: count ? Math.min(10.5, 3.1 + 1.55 * Math.sqrt(count)) : top ? 3 : 2.4,
        glow: count ? 0.62 + 0.38 * warmth : 0.8,
      };
    });
    // Kept in the order their names are drawn, so that where two names would
    // overlap, the top star's or the fuller star's is the one shown.
    bodies.sort((a, b) => b.top - a.top || b.count - a.count);
    this.#bodies = new Map(bodies.map((body) => [body.star.id, body]));
    this.#links = links.map((link) => link.map((id) => this.#bodies.get(id)));
    this.redraw();
  }

  /** Changes what is marked on the map: `focus`, `hover`, `drop` or `rubber`. */
  mark(marks) {
    if (Object.entries(marks).every(([name, value]) => this[name] === value)) return;
    if ("focus" in marks && marks.focus !== this.focus) this.#ringAt = performance.now();
    Object.assign(this, marks);
    this.redraw();
  }

  /** Paints the next frame, and keeps painting for as long as anything moves. */
  redraw() {
    if (this.#frame) return;
    this.#frame = requestAnimationFrame((now) => {
      this.#frame = 0;
      const moving = this.#move(now);
      this.#paint(now);
      if (moving) this.redraw();
      else this.#last = 0;
    });
  }

  // The view

  toWorld(x, y) {
    return [(x - this.view.x) / this.view.k, (y - this.view.y) / this.view.k];
  }

  toScreen(x, y) {
    return [x * this.view.k + this.view.x, y * this.view.k + this.view.y];
  }

  /** ID of the star at a screen point, or null. */
  starAt(x, y) {
    let [found, nearest] = [null, Infinity];
    for (const body of this.#bodies.values()) {
      this.#locate(body);
      const away = Math.hypot(x - body.x, y - body.y);
      if (away <= Math.max(14, body.size + 8) && away < nearest) [found, nearest] = [body.star.id, away];
    }
    return found;
  }

  /** Stops the map where it is, as a press on it does. */
  hold() {
    this.#glide = this.#zoom = this.#fling = this.#pan = null;
  }

  panBy(dx, dy) {
    this.view.x += dx;
    this.view.y += dy;
    // The speed of the last few moves, which the map coasts on at when let go.
    const [now, last] = [performance.now(), this.#pan ?? { vx: 0, vy: 0 }];
    const lapse = Math.max(1, now - (last.at ?? now - 16));
    this.#pan = { at: now, vx: 0.6 * (dx / lapse) + 0.4 * last.vx, vy: 0.6 * (dy / lapse) + 0.4 * last.vy };
    this.redraw();
  }

  /** Lets go of a pan. */
  release() {
    const pan = this.#pan;
    this.#pan = null;
    if (this.still || !pan || performance.now() - pan.at > 60 || Math.hypot(pan.vx, pan.vy) < 0.15) return;
    this.#fling = pan;
    this.redraw();
  }

  /** Zooms by `factor`, keeping the world point under the screen point (x, y) where it is. */
  zoomAt(x, y, factor) {
    this.#glide = this.#fling = null;
    const [wx, wy] = this.toWorld(x, y);
    this.#zoom = { k: clamp((this.#zoom?.k ?? this.view.k) * factor, ...ZOOM), x, y, wx, wy };
    this.redraw();
  }

  /** Moves to another view. */
  glideTo(view) {
    this.hold();
    if (this.still) this.view = view;
    else this.#glide = { from: { ...this.view }, to: view, start: performance.now() };
    this.redraw();
  }

  /** Glides until the world point (x, y) sits at the screen point (sx, sy). */
  bring(x, y, sx, sy) {
    const k = this.view.k;
    this.glideTo({ x: sx - x * k, y: sy - y * k, k });
  }

  /** Shows every star at once, between `left` and `right` on screen. */
  fit(left, right) {
    const stars = [...this.#bodies.values()].map((body) => body.star);
    if (!stars.length) return;
    const [xs, ys] = [stars.map((star) => star.x), stars.map((star) => star.y)];
    const [x0, x1, y0, y1] = [Math.min(...xs), Math.max(...xs), Math.min(...ys), Math.max(...ys)];
    const room = [(right - left - 140) / (x1 - x0 + 1), (this.#height - 160) / (y1 - y0 + 1)];
    const k = clamp(Math.min(...room), ZOOM[0], 1);
    this.glideTo({
      x: (left + right) / 2 - ((x0 + x1) / 2) * k,
      y: this.#height / 2 - ((y0 + y1) / 2) * k,
      k,
    });
  }

  #restore() {
    this.view = { x: this.#width / 2, y: this.#height / 2, k: 1 };
    try {
      const { x, y, k } = JSON.parse(localStorage.getItem(SAVED));
      if (![x, y, k].every(Number.isFinite) || k < ZOOM[0] || k > ZOOM[1]) return;
      [this.view, this.fresh] = [{ x, y, k }, false];
    } catch {
      // An unreadable saved view is the same as none.
    }
  }

  #resize() {
    const ratio = Math.min(2, devicePixelRatio || 1);
    if (ratio !== this.#ratio) this.#dust = DUST.map((layer) => ({ ...layer, tile: dust(layer, ratio) }));
    [this.#ratio, this.#width, this.#height] = [ratio, this.#canvas.clientWidth, this.#canvas.clientHeight];
    this.#canvas.width = Math.round(this.#width * ratio);
    this.#canvas.height = Math.round(this.#height * ratio);
    // Sizing a canvas clears it, so it is painted again before the page is shown.
    this.#paint(performance.now());
  }

  // Motion

  /** Advances whatever is moving. True while anything still is. */
  #move(now) {
    const lapse = this.#last ? Math.min(40, now - this.#last) : 16;
    this.#last = now;
    const view = this.view;
    if (this.#glide) {
      const { from, to, start } = this.#glide;
      const done = clamp((now - start) / GLIDE, 0, 1);
      const ease = 1 - (1 - done) ** 3;
      view.k = from.k * (to.k / from.k) ** ease;
      view.x = from.x + (to.x - from.x) * ease;
      view.y = from.y + (to.y - from.y) * ease;
      if (done === 1) this.#glide = null;
    }
    if (this.#zoom) {
      const { k, x, y, wx, wy } = this.#zoom;
      const closing = this.still ? 1 : 1 - Math.exp(-lapse / 55);
      const next = view.k * (k / view.k) ** closing;
      view.k = Math.abs(k / next - 1) < 0.002 ? k : next;
      [view.x, view.y] = [x - wx * view.k, y - wy * view.k];
      if (view.k === k) this.#zoom = null;
    }
    if (this.#fling) {
      const [fling, slowing] = [this.#fling, Math.exp(-lapse / 280)];
      view.x += fling.vx * lapse;
      view.y += fling.vy * lapse;
      [fling.vx, fling.vy] = [fling.vx * slowing, fling.vy * slowing];
      if (Math.hypot(fling.vx, fling.vy) < 0.02) this.#fling = null;
    }
    const closingRing = !this.still && this.focus !== null && now - this.#ringAt < RING;
    return Boolean(this.#glide || this.#zoom || this.#fling || closingRing);
  }

  // Painting

  /** Works out where a star falls on screen and how large it is drawn. */
  #locate(body) {
    [body.x, body.y] = this.toScreen(body.star.x, body.star.y);
    // Stars swell more slowly than the gaps between them.
    body.size = body.core * this.view.k ** 0.6; // Stars swell more slowly than the gaps between them.
  }

  #paint(now) {
    const [ctx, width, height] = [this.#ctx, this.#width, this.#height];
    ctx.setTransform(this.#ratio, 0, 0, this.#ratio, 0, 0);
    this.#paintBackdrop(ctx, width, height);
    for (const body of this.#bodies.values()) {
      this.#locate(body);
      const reach = body.size * REACH;
      body.seen = body.x > -reach && body.x < width + reach && body.y > -reach && body.y < height + reach;
    }
    this.#paintLinks(ctx);
    this.#paintStars(ctx);
    this.#paintMarks(ctx, now);
    this.#paintNames(ctx);

    const shown = JSON.stringify(this.view);
    if (shown === this.#shown) return;
    this.#shown = shown;
    clearTimeout(this.#saving);
    this.#saving = setTimeout(() => localStorage.setItem(SAVED, shown), 400);
  }

  #paintBackdrop(ctx, width, height) {
    const { x, y, k } = this.view;
    // The world point in the middle of the window, which the backdrop drifts against.
    const [wx, wy] = [(width / 2 - x) / k, (height / 2 - y) / k];

    // The gas slides a little and then no further, however far the map is panned.
    const slack = 140;
    const slide = (far) => slack * Math.tanh((far * 0.04) / slack);
    const gas = this.#gas;
    const scale = Math.max((width + slack * 2) / gas.width, (height + slack * 2) / gas.height);
    const [across, down] = [gas.width * scale, gas.height * scale];
    ctx.drawImage(gas, (width - across) / 2 - slide(wx), (height - down) / 2 - slide(wy), across, down);

    for (const { tile, drift, swell } of this.#dust) {
      const grown = k ** swell;
      const side = TILE * grown;
      const [left, top] = [width / 2 - wx * drift * grown, height / 2 - wy * drift * grown];
      for (let tx = mod(left, side) - side; tx < width; tx += side) {
        for (let ty = mod(top, side) - side; ty < height; ty += side) ctx.drawImage(tile, tx, ty, side, side);
      }
    }
  }

  #paintLinks(ctx) {
    // A link thins from its larger star to its smaller one. Those that touch
    // the focused star are drawn last, in gold.
    for (const near of [false, true]) {
      const [wide, fine] = near ? [0.8, 0.3] : [0.62, 0.22];
      ctx.beginPath();
      for (const [larger, smaller] of this.#links) {
        const touches = larger.star.id === this.focus || smaller.star.id === this.focus;
        if (touches !== near || (!larger.seen && !smaller.seen)) continue;
        const [dx, dy] = [smaller.x - larger.x, smaller.y - larger.y];
        const length = Math.hypot(dx, dy);
        // A link stops short of its stars, and outside the ring around the focused one.
        const clear = (body) => body.size + (body.star.id === this.focus ? 17 : 7);
        const [from, to] = [clear(larger), length - clear(smaller)];
        if (to <= from) continue;
        const [ux, uy] = [dx / length, dy / length];
        const [x1, y1, x2, y2] = [larger.x + ux * from, larger.y + uy * from, larger.x + ux * to, larger.y + uy * to];
        ctx.moveTo(x1 - uy * wide, y1 + ux * wide);
        ctx.lineTo(x2 - uy * fine, y2 + ux * fine);
        ctx.lineTo(x2 + uy * fine, y2 - ux * fine);
        ctx.lineTo(x1 + uy * wide, y1 - ux * wide);
        ctx.closePath();
      }
      ctx.fillStyle = near ? fade(this.#ink.gold, 0.82) : fade(this.#ink.link, 0.32);
      ctx.fill();
    }
  }

  #paintStars(ctx) {
    ctx.globalCompositeOperation = "lighter"; // Light adds up where two stars overlap.
    for (const body of this.#bodies.values()) {
      if (!body.seen) continue;
      const half = body.size * REACH;
      const pixels = clamp(2 ** Math.ceil(Math.log2(half * 2 * this.#ratio)), 32, 512);
      if (body.light?.width !== pixels) body.light = light(body.kind, body.tint, pixels);
      ctx.globalAlpha = body.glow;
      ctx.drawImage(body.light, body.x - half, body.y - half, half * 2, half * 2);
    }
    ctx.globalAlpha = 1;
    ctx.globalCompositeOperation = "source-over";
  }

  #paintMarks(ctx, now) {
    const [focus, drop, rubber] = [this.#bodies.get(this.focus), this.#bodies.get(this.drop), this.rubber];
    ctx.lineWidth = 1;
    if (focus?.seen) {
      // The ring closes in on its star, in four arcs that let the spikes through.
      const ease = this.still ? 1 : 1 - (1 - clamp((now - this.#ringAt) / RING, 0, 1)) ** 3;
      const radius = focus.size + 10 + (1 - ease) * 18;
      ctx.strokeStyle = fade(this.#ink.gold, ease);
      ctx.beginPath();
      for (let arc = 0; arc < 4; arc += 1) {
        const from = (arc * TAU) / 4 + 0.32;
        ctx.moveTo(focus.x + Math.cos(from) * radius, focus.y + Math.sin(from) * radius);
        ctx.arc(focus.x, focus.y, radius, from, from + TAU / 4 - 0.64);
      }
      ctx.stroke();
    }
    ctx.setLineDash([3, 4]);
    if (drop?.seen) {
      ctx.strokeStyle = this.#ink.starlight;
      ctx.beginPath();
      ctx.arc(drop.x, drop.y, drop.size + 10, 0, TAU);
      ctx.stroke();
    }
    const from = this.#bodies.get(rubber?.from);
    if (from) {
      ctx.strokeStyle = this.#ink.gold;
      ctx.beginPath();
      ctx.moveTo(from.x, from.y);
      ctx.lineTo(rubber.x, rubber.y);
      ctx.stroke();
    }
    ctx.setLineDash([]);
  }

  #paintNames(ctx) {
    // Names keep one size on screen. Where two would overlap only the first is
    // drawn, and those of smaller stars fade out as the map shrinks. A star
    // that is focused or pointed at always shows its name.
    const faint = clamp((this.view.k - 0.24) / 0.16, 0, 1);
    const { starlight, gold } = this.#ink;
    const marked = (body) => body.star.id === this.focus || body.star.id === this.hover;
    const bodies = [...this.#bodies.values()].filter((body) => body.seen);
    const taken = []; // Boxes of the names drawn so far.
    let set = ""; // The typeface the canvas was last given.

    ctx.textAlign = "center";
    for (const body of [...bodies.filter(marked), ...bodies.filter((body) => !marked(body))]) {
      const alpha = marked(body) ? 1 : body.top ? 0.92 : 0.76 * faint;
      const font = body.top ? FONT.top : FONT.name;
      if (set !== font) ctx.font = set = font;
      body.wide ??= ctx.measureText(body.star.name).width;
      // The focused star's name and count stand clear of the ring around it.
      body.clear = body.star.id === this.focus ? 8 : 0;
      const [x, y] = [body.x, body.y + body.size + body.clear + (body.top ? 27 : 22)];
      const box = [x - body.wide / 2 - 5, y - 15, x + body.wide / 2 + 5, y + 6];
      const overlaps = (other) => box[0] < other[2] && box[2] > other[0] && box[1] < other[3] && box[3] > other[1];
      const covered = taken.some(overlaps);
      body.named = alpha >= 0.02 && (marked(body) || !covered);
      if (!body.named) continue;
      taken.push(box);
      ctx.globalAlpha = alpha;
      ctx.fillStyle = body.star.id === this.focus ? gold : starlight;
      ctx.fillText(body.star.name, x, y);
    }

    ctx.font = FONT.count;
    ctx.textAlign = "left";
    ctx.fillStyle = gold;
    for (const body of bodies) {
      if (!body.named || !body.count) continue;
      ctx.globalAlpha = marked(body) || body.top ? 0.95 : 0.85 * faint;
      ctx.fillText(body.count, body.x + body.size + body.clear + 5, body.y - body.size - body.clear - 4);
    }
    ctx.globalAlpha = 1;
  }
}
