import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { AppController } from "../../src/ui/app-controller";
import type { TauriBridgeApi, UiSnapshot } from "../../src/ui/contracts";

function createController(
  invoke: TauriBridgeApi["invoke"],
  emit: TauriBridgeApi["event"]["emit"] = vi.fn().mockResolvedValue(undefined),
): { controller: AppController; snapshots: UiSnapshot[] } {
  const snapshots: UiSnapshot[] = [];
  const bridge: TauriBridgeApi = {
    invoke,
    isBrowserMode: () => true,
    event: {
      listen: vi.fn().mockResolvedValue(() => undefined),
      emit,
    },
  };
  vi.stubGlobal("window", {
    TauriBridge: bridge,
    setTimeout: globalThis.setTimeout.bind(globalThis) as unknown as Window["setTimeout"],
    clearTimeout: globalThis.clearTimeout.bind(globalThis) as unknown as Window["clearTimeout"],
    setInterval: globalThis.setInterval.bind(globalThis) as unknown as Window["setInterval"],
    clearInterval: globalThis.clearInterval.bind(globalThis) as unknown as Window["clearInterval"],
  });
  vi.stubGlobal("localStorage", { getItem: vi.fn(() => null), setItem: vi.fn() });
  return { controller: new AppController((snapshot) => snapshots.push(snapshot)), snapshots };
}

describe("AppController settings persistence", () => {
  beforeEach(() => {
    vi.useRealTimers();
  });

  afterEach(() => {
    vi.useRealTimers();
    vi.unstubAllGlobals();
  });

  it.each([
    ["language", (controller: AppController) => controller.setLanguage("source", "ja-JP")],
    [
      "recognition preset",
      (controller: AppController) => controller.setRecognitionPreset("accurate"),
    ],
    ["continuity", (controller: AppController) => controller.setContinuity(true)],
    [
      "preference",
      (controller: AppController) => controller.updatePreference("minimizeToTray", false),
    ],
  ])("surfaces %s save failures without rejecting the update", async (_name, update) => {
    const invoke = vi.fn().mockRejectedValue(new Error("settings unavailable"));
    const { controller, snapshots } = createController(invoke);

    await expect(update(controller)).resolves.toBeUndefined();

    expect(snapshots.at(-1)?.error).toBe("settings unavailable");
    expect(invoke).toHaveBeenCalledWith("save_settings", expect.anything());
  });

  it("surfaces appearance save failures without rejecting the update", async () => {
    vi.useFakeTimers();
    const invoke = vi.fn().mockResolvedValue(undefined);
    const { controller, snapshots } = createController(invoke);
    invoke.mockRejectedValueOnce(new Error("appearance unavailable"));

    await expect(controller.updateOverlay({ fontSize: 40 })).resolves.toBeUndefined();
    await vi.advanceTimersByTimeAsync(250);

    expect(snapshots.at(-1)?.error).toBe("appearance unavailable");
    controller.dispose();
  });

  it("keeps a required start save failure visible and explicit", async () => {
    const invoke = vi.fn().mockRejectedValue(new Error("settings unavailable"));
    const { controller, snapshots } = createController(invoke);

    await expect(controller.start()).resolves.toBeUndefined();

    expect(snapshots.at(-1)).toMatchObject({
      busy: "idle",
      error: "settings unavailable",
      running: false,
    });
    expect(invoke).toHaveBeenCalledTimes(1);
  });
});
