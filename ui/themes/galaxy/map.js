// Galaxy paints the map as a night sky: each domain is a star of light coloured
// by how recently it was worked on, over glowing gas and far stars.

import { clamp, fade } from "../../map.js";

const TAU = Math.PI * 2;
const REACH = 9; // A star's light dies out this many core radii from its middle.
const TILE = 480; // Side of the repeating tile of far stars.

// Blackbody colours from about 3000 K to 20000 K, coolest first. A star worked
// on today burns blue-white, and it cools to amber over three months of quiet.
const HEAT = ["#ffb46b", "#ffd1a3", "#ffe4ce", "#fff3ef", "#e6ebff", "#c9d7ff", "#aabfff"];
const UNLIT = "#b4bfe6"; // A star that holds no sessions yet.

// How long a kind of star's four spikes run, in core radii, and how strong they are.
const SPIKES = { top: [8.8, 1], lit: [6.6, 0.8], unlit: [4.4, 0.45] };

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

const mod = (value, by) => ((value % by) + by) % by;

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

let gas = null; // Painted on first use, as it takes a moment.
let tiles = [];

export default {
  reach: REACH,
  below: { top: 27, name: 22 },

  shape(body) {
    const { count, top, warmth } = body;
    body.kind = !count ? "unlit" : top ? "top" : "lit";
    body.tint = count ? HEAT[Math.round(warmth * (HEAT.length - 1))] : UNLIT;
    body.core = count ? Math.min(10.5, 3.1 + 1.55 * Math.sqrt(count)) : top ? 3 : 2.4;
    body.glow = count ? 0.62 + 0.38 * warmth : 0.8;
  },

  resize(ratio) {
    tiles = DUST.map((layer) => ({ ...layer, tile: dust(layer, ratio) }));
  },

  backdrop({ ctx, width, height, view: { x, y, k } }) {
    gas ??= nebula();
    // The world point in the middle of the window, which the backdrop drifts against.
    const [wx, wy] = [(width / 2 - x) / k, (height / 2 - y) / k];

    // The gas slides a little and then no further, however far the map is panned.
    const slack = 140;
    const slide = (far) => slack * Math.tanh((far * 0.04) / slack);
    const scale = Math.max((width + slack * 2) / gas.width, (height + slack * 2) / gas.height);
    const [across, down] = [gas.width * scale, gas.height * scale];
    ctx.drawImage(gas, (width - across) / 2 - slide(wx), (height - down) / 2 - slide(wy), across, down);

    for (const { tile, drift, swell } of tiles) {
      const grown = k ** swell;
      const side = TILE * grown;
      const [left, top] = [width / 2 - wx * drift * grown, height / 2 - wy * drift * grown];
      for (let tx = mod(left, side) - side; tx < width; tx += side) {
        for (let ty = mod(top, side) - side; ty < height; ty += side) ctx.drawImage(tile, tx, ty, side, side);
      }
    }
  },

  links({ ctx, ink, links, focus }) {
    // A link thins from its parent to its subdomain. Those that touch the
    // focused star are drawn last, in the accent.
    for (const near of [false, true]) {
      const [wide, fine] = near ? [0.8, 0.3] : [0.62, 0.22];
      ctx.beginPath();
      for (const { from, to } of links) {
        if ((from.domain.id === focus || to.domain.id === focus) !== near) continue;
        const [dx, dy] = [to.x - from.x, to.y - from.y];
        const length = Math.hypot(dx, dy);
        // A link stops short of its stars, and outside the ring around the focused one.
        const clear = (body) => body.size + (body.domain.id === focus ? 17 : 7);
        const [start, end] = [clear(from), length - clear(to)];
        if (end <= start) continue;
        const [ux, uy] = [dx / length, dy / length];
        const [x1, y1, x2, y2] = [from.x + ux * start, from.y + uy * start, from.x + ux * end, from.y + uy * end];
        ctx.moveTo(x1 - uy * wide, y1 + ux * wide);
        ctx.lineTo(x2 - uy * fine, y2 + ux * fine);
        ctx.lineTo(x2 + uy * fine, y2 - ux * fine);
        ctx.lineTo(x1 + uy * wide, y1 - ux * wide);
        ctx.closePath();
      }
      ctx.fillStyle = near ? fade(ink.accent, 0.82) : fade(ink.link, 0.32);
      ctx.fill();
    }
  },

  domains({ ctx, ratio, bodies }) {
    ctx.globalCompositeOperation = "lighter"; // Light adds up where two stars overlap.
    for (const body of bodies) {
      const half = body.size * REACH;
      const pixels = clamp(2 ** Math.ceil(Math.log2(half * 2 * ratio)), 32, 512);
      if (body.light?.width !== pixels) body.light = light(body.kind, body.tint, pixels);
      ctx.globalAlpha = body.glow;
      ctx.drawImage(body.light, body.x - half, body.y - half, half * 2, half * 2);
    }
    ctx.globalAlpha = 1;
    ctx.globalCompositeOperation = "source-over";
  },

  /** The focus ring, in four arcs that let the spikes through. */
  ring(ctx, body, radius, { ink }) {
    ctx.strokeStyle = ink.accent;
    ctx.beginPath();
    for (let arc = 0; arc < 4; arc += 1) {
      const from = (arc * TAU) / 4 + 0.32;
      ctx.moveTo(body.x + Math.cos(from) * (radius + 2), body.y + Math.sin(from) * (radius + 2));
      ctx.arc(body.x, body.y, radius + 2, from, from + TAU / 4 - 0.64);
    }
    ctx.stroke();
  },
};
