import { afterEach, beforeEach, expect, it, vi } from "vitest";
import type { UiSnapshot } from "../../src/ui/contracts";
import { DisplayController } from "../../src/ui/display-controller";

const region = { x: 100, y: 200, width: 600, height: 80 };
let listeners: Map<string, (event: { payload: unknown }) => void>;
let invoke: ReturnType<typeof vi.fn>;
let publish = vi.fn<(patch: Partial<UiSnapshot>) => void>();
let controller: DisplayController;
let browserMode: boolean;

beforeEach(() => {
  vi.useFakeTimers();
  listeners = new Map();
  invoke = vi.fn().mockResolvedValue({ multipleDisplays: true, region });
  publish = vi.fn();
  browserMode = false;
  vi.stubGlobal("window", {
    setTimeout: globalThis.setTimeout,
    clearTimeout: globalThis.clearTimeout,
    TauriBridge: {
      isBrowserMode: () => browserMode,
      invoke,
      event: {
        listen: vi.fn(async (name, callback) => {
          listeners.set(name, callback);
          return () => listeners.delete(name);
        }),
      },
    },
  });
  controller = new DisplayController(publish);
});

afterEach(() => {
  controller.dispose();
  vi.useRealTimers();
  vi.unstubAllGlobals();
});

it("uses the native display context, including a cleared area on another screen", async () => {
  expect(await controller.read()).toEqual({ multipleDisplays: true, region });
  invoke.mockResolvedValue({ multipleDisplays: false, region: null });
  expect(await controller.read()).toEqual({ multipleDisplays: false, region: null });
});

it("does not invent a display in browser mode or on discovery failure", async () => {
  browserMode = true;
  expect(await controller.read()).toEqual({});
  expect(invoke).not.toHaveBeenCalled();
  browserMode = false;
  invoke.mockResolvedValue(undefined);
  expect(await controller.read()).toEqual({});
  invoke.mockRejectedValue(new Error("No display"));
  expect(await controller.read()).toEqual({});
});

it("debounces main-window moves without probing or preparing the engine", async () => {
  await controller.start();
  listeners.get("display-context-changed")?.({ payload: null });
  await vi.advanceTimersByTimeAsync(100);
  listeners.get("display-context-changed")?.({ payload: null });
  await vi.advanceTimersByTimeAsync(149);
  expect(invoke).not.toHaveBeenCalled();
  await vi.advanceTimersByTimeAsync(1);
  expect(invoke).toHaveBeenCalledExactlyOnceWith("get_display_context");
  expect(publish).toHaveBeenCalledWith({ multipleDisplays: true, region });
});

it("requires selection after a display change and never restarts translation", async () => {
  await controller.start();
  listeners.get("display-invalidated")?.({
    payload: "The display changed. Select the subtitle area again.",
  });
  expect(publish).toHaveBeenCalledWith({
    running: false,
    busy: "idle",
    region: null,
    captureWarning: null,
    notice: null,
    error: "The display changed. Select the subtitle area again.",
  });
  expect(invoke).not.toHaveBeenCalled();
});

it("releases listeners and cancels pending display refreshes on disposal", async () => {
  await controller.start();
  listeners.get("display-context-changed")?.({ payload: null });
  controller.dispose();
  await vi.advanceTimersByTimeAsync(200);
  expect(listeners.size).toBe(0);
  expect(invoke).not.toHaveBeenCalled();
});

it("drops a display response that completes after disposal", async () => {
  let resolve: (value: unknown) => void = () => {};
  invoke.mockReturnValue(
    new Promise((done) => {
      resolve = done;
    }),
  );
  await controller.start();
  listeners.get("display-context-changed")?.({ payload: null });
  await vi.advanceTimersByTimeAsync(150);
  controller.dispose();
  resolve({ multipleDisplays: true, region });
  await Promise.resolve();
  await Promise.resolve();
  expect(publish).not.toHaveBeenCalled();
});

it("cannot restore a lost selection from a display response already in flight", async () => {
  let resolve: (value: unknown) => void = () => {};
  invoke.mockReturnValue(
    new Promise((done) => {
      resolve = done;
    }),
  );
  await controller.start();
  listeners.get("display-context-changed")?.({ payload: null });
  await vi.advanceTimersByTimeAsync(150);
  listeners.get("display-invalidated")?.({ payload: "Display disconnected" });
  resolve({ multipleDisplays: true, region });
  await Promise.resolve();
  await Promise.resolve();
  expect(publish).toHaveBeenCalledTimes(1);
  expect(publish).toHaveBeenLastCalledWith(
    expect.objectContaining({ region: null, running: false }),
  );
});

it("a display-loss notice cancels an unstarted refresh", async () => {
  await controller.start();
  listeners.get("display-context-changed")?.({ payload: null });
  listeners.get("display-invalidated")?.({ payload: "Display disconnected" });
  await vi.advanceTimersByTimeAsync(150);
  expect(invoke).not.toHaveBeenCalled();
});
