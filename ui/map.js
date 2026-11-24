// The map: every domain, link and name on one canvas. Panning or zooming
// repaints a frame and builds no elements, and nothing is painted while the map
// is still. This file owns the view, its motion and where things are; a theme's
// painter decides how domains, links and the backdrop look (see
// `docs/themes.md`). It knows nothing of sessions or requests: `app.js` hands it
// domains with their links and asks it where things are.

const TAU = Math.PI * 2;
const SAVED = "noema-deck-view"; // Where the pan and zoom are kept between runs.
const ZOOM = [0.2, 3]; // The smallest and largest scale the map takes.
const GLIDE = 460; // Milliseconds a glide to a new view takes.
const RING = 320; // Milliseconds the focus ring takes to close on its domain.

// The colours and typefaces a theme sets for the map, read from its tokens.
// Colours are six-digit, so a painter can add an alpha to them.
const INKS = ["canvas", "ink", "accent", "link", "node", "faded", "live", "name", "top", "count"];

export const clamp = (value, least, most) => Math.min(most, Math.max(least, value));
/** A six-digit colour at `alpha`, spelled the way a canvas takes it. */
export const fade = (colour, alpha) => colour + Math.round(clamp(alpha, 0, 1) * 255).toString(16).padStart(2, "0");

/** 1 for a domain worked on today, falling to 0 over three months of quiet. */
function heat(updated) {
  if (!updated) return 0;
  const days = Math.max(0, Date.now() / 1000 - updated) / 86400;
  return Math.max(0, 1 - Math.log1p(days) / Math.log1p(90));
}

/** The plain focus ring, closing in on its domain as `ease` goes from 0 to 1. */
function ring(ctx, body, radius, scene) {
  ctx.strokeStyle = scene.ink.accent;
  ctx.beginPath();
  ctx.arc(body.x, body.y, radius, 0, TAU);
  ctx.stroke();
}

export class MapView {
  view = { x: 0, y: 0, k: 1 }; // A world point (x, y) is drawn at x * k + view.x.
  fresh = true; // Whether no earlier run left a view behind.
  still = matchMedia("(prefers-reduced-motion: reduce)").matches; // Whether every move is a jump.
  focus = null; // ID of the focused domain.
  hover = null; // ID of the domain under the pointer.
  drop = null; // ID of the domain a drag would land on.
  rubber = null; // A link being drawn: `{ from: domain ID, x, y }`, ending at a screen point.

  #canvas;
  #ctx;
  #painter = null;
  #ink = {};
  #ratio = 0;
  #width = 0;
  #height = 0;
  #domains = [];
  #domainLinks = [];
  #bodies = new Map();
  #links = [];
  #live = new Set();
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
    new ResizeObserver(() => this.#resize()).observe(canvas);
    this.#resize();
    this.#restore();
  }

  // The scene

  /**
   * Takes a theme's painter and reads the map's colours and typefaces from the
   * page's tokens, as they stand once the theme's style sheet has loaded.
   */
  paintWith(painter) {
    this.#painter = painter;
    const style = getComputedStyle(document.documentElement);
    for (const name of INKS) this.#ink[name] = style.getPropertyValue(`--map-${name}`).trim();
    painter.resize?.(this.#ratio);
    this.chart(this.#domains, this.#domainLinks);
    // A canvas does not ask for a typeface, so it is asked for here, and names
    // measured before it arrives are measured again.
    for (const font of [this.#ink.name, this.#ink.top, this.#ink.count]) {
      document.fonts.load(font).then(() => {
        for (const body of this.#bodies.values()) body.wide = undefined;
        this.redraw();
      });
    }
  }

  /**
   * Takes the domains and links to draw. A domain's place is read each time it
   * is painted, so a domain being dragged follows the pointer.
   */
  chart(domains, links) {
    [this.#domains, this.#domainLinks] = [domains, links];
    const hanging = new Set(links.map(([, child]) => child));
    const bodies = domains.map((domain) => ({
      domain,
      top: !hanging.has(domain.id),
      count: domain.sessions.length,
      warmth: heat(domain.updated),
      live: this.#live.has(domain.id),
    }));
    // Kept in the order their names are drawn, so that where two names would
    // overlap, the root domain's or the fuller domain's is the one shown.
    bodies.sort((a, b) => b.top - a.top || b.count - a.count);
    this.#bodies = new Map(bodies.map((body) => [body.domain.id, body]));
    for (const body of bodies) this.#painter?.shape(body);

    // A link to a subdomain that already has a parent is its second link.
    const parented = new Set();
    this.#links = links.map(([parent, child]) => {
      const [from, to] = [this.#bodies.get(parent), this.#bodies.get(child)];
      const second = parented.has(child);
      parented.add(child);
      return { from, to, second };
    }).filter(({ from, to }) => from && to);
    this.redraw();
  }

  /** Marks the domains holding a session that is running now. */
  light(ids) {
    const live = new Set(ids);
    if (live.size === this.#live.size && [...live].every((id) => this.#live.has(id))) return;
    this.#live = live;
    for (const body of this.#bodies.values()) body.live = live.has(body.domain.id);
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

  /** ID of the domain at a screen point, or null. */
  domainAt(x, y) {
    let [found, nearest] = [null, Infinity];
    for (const body of this.#bodies.values()) {
      this.#locate(body);
      const away = Math.hypot(x - body.x, y - body.y);
      if (away <= Math.max(14, body.size + 8) && away < nearest) [found, nearest] = [body.domain.id, away];
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

  /** Shows every domain at once, between `left` and `right` on screen. */
  fit(left, right) {
    const domains = [...this.#bodies.values()].map((body) => body.domain);
    if (!domains.length) return;
    const [xs, ys] = [domains.map((domain) => domain.x), domains.map((domain) => domain.y)];
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
    if (ratio !== this.#ratio) this.#painter?.resize?.(ratio);
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

  /** Works out where a domain falls on screen and how large it is drawn. */
  #locate(body) {
    [body.x, body.y] = this.toScreen(body.domain.x, body.domain.y);
    body.size = (body.core ?? 3) * this.view.k ** 0.6; // Domains swell more slowly than the gaps between them.
  }

  #paint(now) {
    const [ctx, width, height, painter] = [this.#ctx, this.#width, this.#height, this.#painter];
    ctx.setTransform(this.#ratio, 0, 0, this.#ratio, 0, 0);
    if (!painter) return;

    // With a domain under the pointer, the painter may dim all but it and its neighbours.
    const near = new Set(this.hover === null ? [] : [this.hover]);
    for (const { from, to } of this.#links) {
      if (from.domain.id === this.hover) near.add(to.domain.id);
      if (to.domain.id === this.hover) near.add(from.domain.id);
    }
    const reach = painter.reach ?? 1;
    const bodies = [];
    for (const body of this.#bodies.values()) {
      this.#locate(body);
      const far = body.size * reach + 4;
      body.seen = body.x > -far && body.x < width + far && body.y > -far && body.y < height + far;
      body.dim = near.size > 0 && !near.has(body.domain.id);
      if (body.seen) bodies.push(body);
    }
    const scene = {
      ctx,
      width,
      height,
      ratio: this.#ratio,
      view: this.view,
      ink: this.#ink,
      bodies,
      links: this.#links.filter(({ from, to }) => from.seen || to.seen),
      focus: this.focus,
      hover: this.hover,
    };
    painter.backdrop(scene);
    painter.links(scene);
    painter.domains(scene);
    this.#paintMarks(scene, now);
    this.#paintNames(scene);

    const shown = JSON.stringify(this.view);
    if (shown === this.#shown) return;
    this.#shown = shown;
    clearTimeout(this.#saving);
    this.#saving = setTimeout(() => localStorage.setItem(SAVED, shown), 400);
  }

  #paintMarks(scene, now) {
    const { ctx, ink } = scene;
    const [focus, drop, rubber] = [this.#bodies.get(this.focus), this.#bodies.get(this.drop), this.rubber];
    ctx.lineWidth = 1;
    if (focus?.seen) {
      const ease = this.still ? 1 : 1 - (1 - clamp((now - this.#ringAt) / RING, 0, 1)) ** 3;
      ctx.globalAlpha = ease;
      (this.#painter.ring ?? ring)(ctx, focus, focus.size + 8 + (1 - ease) * 18, scene);
      ctx.globalAlpha = 1;
    }
    ctx.setLineDash([3, 4]);
    if (drop?.seen) {
      ctx.strokeStyle = ink.ink;
      ctx.beginPath();
      ctx.arc(drop.x, drop.y, drop.size + 10, 0, TAU);
      ctx.stroke();
    }
    const from = this.#bodies.get(rubber?.from);
    if (from) {
      ctx.strokeStyle = ink.accent;
      ctx.beginPath();
      ctx.moveTo(from.x, from.y);
      ctx.lineTo(rubber.x, rubber.y);
      ctx.stroke();
    }
    ctx.setLineDash([]);
  }

  #paintNames({ ctx, ink, bodies }) {
    // Names keep one size on screen. Where two would overlap only the first is
    // drawn, and those of subdomains fade out as the map shrinks. A domain that
    // is focused or pointed at always shows its name.
    const faint = clamp((this.view.k - 0.24) / 0.16, 0, 1);
    const below = this.#painter.below ?? { top: 18, name: 15 };
    const dims = this.#painter.dims;
    const marked = (body) => body.domain.id === this.focus || body.domain.id === this.hover;
    const shade = (body) => (dims && body.dim ? 0.3 : 1);
    const taken = []; // Boxes of the names drawn so far.
    let set = ""; // The typeface the canvas was last given.

    ctx.textAlign = "center";
    for (const body of [...bodies.filter(marked), ...bodies.filter((body) => !marked(body))]) {
      const alpha = (marked(body) ? 1 : body.top ? 0.92 : 0.76 * faint) * shade(body);
      const font = body.top ? ink.top : ink.name;
      if (set !== font) ctx.font = set = font;
      body.wide ??= ctx.measureText(body.domain.name).width;
      // The focused domain's name and count stand clear of the ring around it.
      body.clear = body.domain.id === this.focus ? 6 : 0;
      const [x, y] = [body.x, body.y + body.size + body.clear + (body.top ? below.top : below.name)];
      const box = [x - body.wide / 2 - 5, y - 15, x + body.wide / 2 + 5, y + 6];
      const overlaps = (other) => box[0] < other[2] && box[2] > other[0] && box[1] < other[3] && box[3] > other[1];
      body.named = alpha >= 0.02 && (marked(body) || !taken.some(overlaps));
      if (!body.named) continue;
      taken.push(box);
      ctx.globalAlpha = alpha;
      ctx.fillStyle = body.domain.id === this.focus ? ink.accent : ink.ink;
      ctx.fillText(body.domain.name, x, y);
    }

    ctx.font = ink.count;
    ctx.textAlign = "left";
    ctx.fillStyle = ink.accent;
    for (const body of bodies) {
      if (!body.named || !body.count) continue;
      ctx.globalAlpha = (marked(body) || body.top ? 0.95 : 0.85 * faint) * shade(body);
      ctx.fillText(body.count, body.x + body.size + body.clear + 5, body.y - body.size - body.clear - 4);
    }
    ctx.globalAlpha = 1;
  }
}
