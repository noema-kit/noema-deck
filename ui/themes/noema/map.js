// Noema paints the map as a graph in ink, after the graph in the Noema Kit
// mark: each domain a dot, each link a hairline, on a plain ground.

const TAU = Math.PI * 2;

/** The colour `t` of the way from `a` to `b`, both six-digit. */
function mix(a, b, t) {
  const channel = (colour, at) => parseInt(colour.slice(1 + at, 3 + at), 16);
  return `#${[0, 2, 4]
    .map((at) => Math.round(channel(a, at) * (1 - t) + channel(b, at) * t).toString(16).padStart(2, "0"))
    .join("")}`;
}

export default {
  reach: 2, // Nothing drawn for a domain strays further than twice its size.
  below: { top: 20, name: 15 }, // How far below a dot its name sits.
  dims: true, // A domain under the pointer dims all but it and its neighbours.

  shape(body) {
    const { count, top } = body;
    body.core = (count ? Math.min(8, 2.6 + 1.1 * Math.sqrt(count)) : 2.6) + (top ? 1.2 : 0);
  },

  backdrop({ ctx, width, height, ink }) {
    ctx.fillStyle = ink.canvas;
    ctx.fillRect(0, 0, width, height);
  },

  links({ ctx, ink, links, focus, hover }) {
    ctx.lineWidth = 1;
    for (const { from, to, second } of links) {
      const touches = (id) => from.domain.id === id || to.domain.id === id;
      const [dx, dy] = [to.x - from.x, to.y - from.y];
      const length = Math.hypot(dx, dy);
      const [start, end] = [from.size + 4, length - to.size - 4];
      if (end <= start) continue;
      const [ux, uy] = [dx / length, dy / length];
      ctx.globalAlpha = hover !== null && !touches(hover) ? 0.25 : 1;
      ctx.strokeStyle = touches(focus) || touches(hover) ? ink.accent : ink.link;
      ctx.setLineDash(second ? [3, 3] : []);
      ctx.beginPath();
      ctx.moveTo(from.x + ux * start, from.y + uy * start);
      ctx.lineTo(from.x + ux * end, from.y + uy * end);
      ctx.stroke();
    }
    ctx.setLineDash([]);
    ctx.globalAlpha = 1;
  },

  domains({ ctx, ink, bodies }) {
    for (const body of bodies) {
      const { x, y, size, count, top, warmth, live, dim } = body;
      // Ink thins from the full colour of today's work to mist over three months.
      const colour = mix(ink.faded, ink.node, count ? 0.35 + 0.65 * warmth : 0.3);
      ctx.globalAlpha = dim ? 0.25 : 1;
      ctx.beginPath();
      ctx.arc(x, y, size, 0, TAU);
      if (count) {
        ctx.fillStyle = colour;
        ctx.fill();
      } else {
        // A domain that holds no sessions yet is an open ring.
        ctx.fillStyle = ink.canvas;
        ctx.fill();
        ctx.lineWidth = 1.2;
        ctx.strokeStyle = colour;
        ctx.stroke();
      }
      ctx.lineWidth = 1;
      if (top) {
        ctx.strokeStyle = colour;
        ctx.beginPath();
        ctx.arc(x, y, size + 3.5, 0, TAU);
        ctx.stroke();
      }
      if (live) {
        // A session runs here now.
        ctx.strokeStyle = ink.live;
        ctx.lineWidth = 1.4;
        ctx.beginPath();
        ctx.arc(x, y, size + (top ? 7 : 4.5), 0, TAU);
        ctx.stroke();
      }
    }
    ctx.globalAlpha = 1;
    ctx.lineWidth = 1;
  },
};
