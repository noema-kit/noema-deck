// What the program in a pane says it is doing, by the Program Status
// Protocol (OSC 7501): idle, working, blocked on the person, done or failed.
// Claude Code and Antigravity report this to a terminal that answers their
// question, so Deck need not guess from what they print.

/** What a terminal answers a program that asks whether it reads reports. */
export const KNOWN = "\x1b]7501;?\x1b\\";

const STATES = new Set(["idle", "working", "blocked", "done", "error"]);
const KINDS = new Set(["permission", "question", "auth"]);
const ID = /^[A-Za-z0-9_.+-]{1,32}(\/[A-Za-z0-9_.+-]{1,32}){0,7}$/;
const VALUE = /^[A-Za-z0-9_.,+/=-]*$/;
const LONGEST = { report: 4087, id: 128, message: 2732 }; // Bytes, as the protocol bounds them.
const HELD = 64; // Records kept for one pane.

/** The text a report carries as base64, or nothing when it is not text to show. */
function text(value) {
  try {
    const bytes = Uint8Array.from(atob(value), (char) => char.charCodeAt(0));
    const read = new TextDecoder("utf-8", { fatal: true }).decode(bytes);
    return /\p{Cc}/u.test(read) ? undefined : read;
  } catch {
    return undefined;
  }
}

/**
 * Reads the body of one report: `{ id, state, kind, message }`, where `state`
 * may also be "clear". Nothing comes back for a report to ignore, which is
 * one without a state Deck knows, or with a record name or a message it
 * cannot read.
 */
export function read(body) {
  if (body.length > LONGEST.report) return null;
  const said = {};
  for (const pair of body.split(":")) {
    const cut = pair.indexOf("=");
    if (cut < 0) continue;
    const [key, value] = [pair.slice(0, cut).trim(), pair.slice(cut + 1).trim()];
    if (VALUE.test(value)) said[key] = value;
    else if (key === "id" || key === "msg") return null;
  }
  const { id = "", state, kind, msg } = said;
  if (state !== "clear" && !STATES.has(state)) return null;
  if (id && (id.length > LONGEST.id || !ID.test(id))) return null;
  const message = msg ? text(msg) : "";
  if (message === undefined || msg?.length > LONGEST.message) return null;
  return { id, state, kind: state === "blocked" && KINDS.has(kind) ? kind : undefined, message };
}

/** The reports of one pane's programs, a record to a name. */
export class Status {
  #records = new Map();

  /** Takes a report's body. Answers whether anything changed. */
  take(body) {
    const report = read(body);
    if (!report) return false;
    const { id, state } = report;
    if (state === "clear") {
      // Clearing a record clears those beneath it, and clearing without a name clears all.
      for (const name of [...this.#records.keys()]) if (!id || name === id || name.startsWith(`${id}/`)) this.#records.delete(name);
      return true;
    }
    this.#records.delete(id);
    this.#records.set(id, report);
    if (this.#records.size > HELD) this.#records.delete(this.#records.keys().next().value);
    return true;
  }

  /** Forgets what was under way: its program ended, or handed the terminal back. */
  rest() {
    for (const [name, { state }] of this.#records) if (state === "working" || state === "blocked") this.#records.delete(name);
  }

  clear() {
    this.#records.clear();
  }

  /** The report that matters most: one waiting on the person, then one at work, then one finished. */
  get now() {
    const order = ["blocked", "working", "error", "done", "idle"];
    const records = [...this.#records.values()];
    return order.map((state) => records.findLast((record) => record.state === state)).find(Boolean) ?? null;
  }
}

const WAITS = { permission: "Waiting for your permission", question: "Waiting for your answer", auth: "Waiting for you to sign in" };

/** What a blocked report waits for, in words. */
export const waits = ({ kind, message }) => [WAITS[kind] ?? "Waiting for you", message].filter(Boolean).join(": ");
