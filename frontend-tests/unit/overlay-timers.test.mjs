import { createRequire } from "node:module";
import { beforeEach, describe, expect, it } from "vitest";

const require = createRequire(import.meta.url);
const { createTimerOwner } = require("../../src/scripts/overlay-timers.js");

// Minimal deterministic clock: ids are handed out in order, nothing fires until
// `fire` is called, and cancelled ids are recorded so cleanup can be asserted.
function createFakeClock() {
  const pending = new Map();
  const cleared = [];
  let nextId = 1;

  const schedule = (kind, callback) => {
    const id = nextId++;
    pending.set(id, { callback, kind });
    return id;
  };

  return {
    cleared,
    pending,
    setTimeout: (callback) => schedule("timeout", callback),
    setInterval: (callback) => schedule("interval", callback),
    clearTimeout: (id) => {
      cleared.push(id);
      pending.delete(id);
    },
    clearInterval: (id) => {
      cleared.push(id);
      pending.delete(id);
    },
    fire(id) {
      const slot = pending.get(id);
      if (!slot) throw new Error(`timer ${id} is not pending`);
      if (slot.kind === "timeout") pending.delete(id);
      slot.callback();
    },
  };
}

describe("overlay timer owner", () => {
  let clock;
  let timers;

  beforeEach(() => {
    clock = createFakeClock();
    timers = createTimerOwner(clock);
  });

  it("reports a scheduled slot as pending and a cancelled one as not", () => {
    timers.timeout("frameFade", () => {}, 4000);
    expect(timers.isPending("frameFade")).toBe(true);

    timers.cancel("frameFade");
    expect(timers.isPending("frameFade")).toBe(false);
    expect(clock.cleared).toEqual([1]);
  });

  it("cancelling an empty or already-cancelled slot is a no-op", () => {
    timers.cancel("frameFade");
    timers.timeout("frameFade", () => {}, 4000);
    timers.cancel("frameFade");
    timers.cancel("frameFade");

    expect(clock.cleared).toEqual([1]);
  });

  it("never runs a cancelled callback", () => {
    let fired = 0;
    timers.timeout("hideCleanup", () => (fired += 1), 220);
    timers.cancel("hideCleanup");

    expect(clock.pending.size).toBe(0);
    expect(fired).toBe(0);
  });

  it("replaces an occupied slot instead of leaking the previous timer", () => {
    let first = 0;
    let second = 0;
    timers.timeout("frameFade", () => (first += 1), 4000);
    timers.timeout("frameFade", () => (second += 1), 4000);

    expect(clock.cleared).toEqual([1]);
    expect(clock.pending.size).toBe(1);

    clock.fire(2);
    expect(first).toBe(0);
    expect(second).toBe(1);
  });

  it("clears a timeout slot when it fires, so isPending never reports a dead timer", () => {
    timers.timeout("hideCleanup", () => {}, 220);
    clock.fire(1);

    expect(timers.isPending("hideCleanup")).toBe(false);
  });

  it("keeps an interval slot pending across ticks until it is cancelled", () => {
    let ticks = 0;
    timers.interval("clickThrough", () => (ticks += 1), 150);

    clock.fire(1);
    clock.fire(1);
    expect(ticks).toBe(2);
    expect(timers.isPending("clickThrough")).toBe(true);

    timers.cancel("clickThrough");
    expect(timers.isPending("clickThrough")).toBe(false);
    expect(clock.pending.size).toBe(0);
  });

  it("keeps slots independent", () => {
    timers.interval("clickThrough", () => {}, 150);
    timers.timeout("frameFade", () => {}, 4000);
    timers.timeout("hideCleanup", () => {}, 220);

    timers.cancel("frameFade");

    expect(timers.isPending("clickThrough")).toBe(true);
    expect(timers.isPending("frameFade")).toBe(false);
    expect(timers.isPending("hideCleanup")).toBe(true);
  });

  it("cancelAll clears every slot", () => {
    timers.interval("clickThrough", () => {}, 150);
    timers.timeout("frameFade", () => {}, 4000);
    timers.timeout("hideCleanup", () => {}, 220);

    timers.cancelAll();

    expect(clock.pending.size).toBe(0);
    expect(timers.isPending("clickThrough")).toBe(false);
    expect(timers.isPending("frameFade")).toBe(false);
    expect(timers.isPending("hideCleanup")).toBe(false);
  });

  it("a restart inside the fade window cannot let the previous stop's cleanup fire", () => {
    // This is the overlay's Stop -> Start sequence: the hide cleanup is armed on
    // stop and must be cancelled by the next show, or it hides the capture frame
    // that was just re-shown.
    let hidden = 0;
    timers.timeout("hideCleanup", () => (hidden += 1), 220);
    timers.cancel("hideCleanup");

    expect(clock.pending.size).toBe(0);
    expect(hidden).toBe(0);
  });
});
