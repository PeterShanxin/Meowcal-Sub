import { afterEach, describe, expect, it, vi } from "vitest";

const modulePath = "../../src/scripts/overlay-liveness.js";

async function loadModule(emitImpl) {
  vi.resetModules();
  vi.stubGlobal("window", { __TAURI__: { event: { emit: emitImpl } } });
  await import(modulePath);
  return globalThis.window.OverlayLiveness;
}

describe("overlay liveness contract", () => {
  afterEach(() => {
    vi.unstubAllGlobals();
    vi.useRealTimers();
  });

  it("emits overlay-ready exactly once after listeners are registered", async () => {
    vi.useFakeTimers();
    const emit = vi.fn(() => Promise.resolve());
    const api = await loadModule(emit);

    api.signalReady();
    api.signalReady();

    const readyEmits = emit.mock.calls.filter(([name]) => name === "overlay-ready");
    expect(readyEmits).toHaveLength(1);
  });

  it("emits overlay-heartbeat on the heartbeat interval", async () => {
    vi.useFakeTimers();
    const emit = vi.fn(() => Promise.resolve());
    const api = await loadModule(emit);

    api.signalReady();
    expect(emit.mock.calls.filter(([name]) => name === "overlay-heartbeat")).toHaveLength(0);

    await vi.advanceTimersByTimeAsync(api.HEARTBEAT_INTERVAL_MS * 2);
    const beats = emit.mock.calls.filter(([name]) => name === "overlay-heartbeat");
    expect(beats).toHaveLength(2);
  });

  it("no-ops without a Tauri bridge (browser mode contract only)", async () => {
    vi.resetModules();
    vi.stubGlobal("window", {});
    const api = await import(modulePath).then(() => globalThis.window.OverlayLiveness);

    api.signalReady();

    // Nothing to assert against a bridge that does not exist: this proves the
    // overlay page does not crash outside Tauri. It does NOT prove native
    // WebView2 recovery - only the Rust-side stale detection does that.
    expect(api.HEARTBEAT_INTERVAL_MS).toBeGreaterThan(0);
  });

  it("keeps beating even when emits fail, and ready stays one-shot", async () => {
    vi.useFakeTimers();
    const warn = vi.spyOn(console, "warn").mockImplementation(() => {});
    const emit = vi.fn(() => Promise.reject(new Error("ipc down")));
    const api = await loadModule(emit);

    api.signalReady();
    api.signalReady();
    await vi.advanceTimersByTimeAsync(api.HEARTBEAT_INTERVAL_MS * 2);

    expect(emit.mock.calls.filter(([name]) => name === "overlay-ready")).toHaveLength(1);
    expect(emit.mock.calls.filter(([name]) => name === "overlay-heartbeat")).toHaveLength(2);
    expect(warn).toHaveBeenCalled();
  });
});
