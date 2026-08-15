/* global module */

// Named-slot owner for the overlay's state-scoped timers.
//
// Three overlay timers belong to overlay *state*, not to the page: the
// click-through poll, the capture-frame fade, and the post-fade hide cleanup.
// Each used to be a loose module variable cleared at its call sites, and the
// hide cleanup was not tracked at all - so stopping and restarting translation
// inside the 220ms fade let the previous stop's cleanup fire onto the freshly
// shown overlay and hide the capture frame for the rest of the session.
//
// One slot per name. Scheduling into an occupied slot replaces what was there,
// cancelling is idempotent, and a fired timeout clears its own slot so
// `isPending` never reports a timer that has already run.
//
// The clock is injectable so the rules can be characterized without waiting.
(function exposeOverlayTimers(root) {
  function createTimerOwner(clock) {
    const timers = clock || root;
    const slots = new Map();

    function cancel(name) {
      const slot = slots.get(name);
      if (!slot) return;

      slots.delete(name);
      if (slot.kind === "interval") {
        timers.clearInterval(slot.id);
      } else {
        timers.clearTimeout(slot.id);
      }
    }

    function interval(name, callback, intervalMs) {
      cancel(name);
      slots.set(name, { kind: "interval", id: timers.setInterval(callback, intervalMs) });
    }

    function timeout(name, callback, delayMs) {
      cancel(name);
      const id = timers.setTimeout(() => {
        slots.delete(name);
        callback();
      }, delayMs);
      slots.set(name, { kind: "timeout", id });
    }

    function isPending(name) {
      return slots.has(name);
    }

    function cancelAll() {
      Array.from(slots.keys()).forEach(cancel);
    }

    return { cancel, cancelAll, interval, isPending, timeout };
  }

  const api = { createTimerOwner };
  if (typeof module === "object" && module.exports) {
    module.exports = api;
  }
  if (root) {
    root.OverlayTimers = api;
  }
})(typeof globalThis !== "undefined" ? globalThis : this);
