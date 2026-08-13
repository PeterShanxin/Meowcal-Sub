import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { AppController } from "../../src/ui/app-controller";
import type { TauriBridgeApi, UiSnapshot } from "../../src/ui/contracts";

function createController(
  invoke: TauriBridgeApi["invoke"],
  emit: TauriBridgeApi["event"]["emit"] = vi.fn().mockResolvedValue(undefined),
  browserMode = true,
): {
  controller: AppController;
  snapshots: UiSnapshot[];
  listeners: Map<string, (event: { payload: unknown }) => void>;
  storage: { getItem: ReturnType<typeof vi.fn>; setItem: ReturnType<typeof vi.fn> };
} {
  const snapshots: UiSnapshot[] = [];
  const listeners = new Map<string, (event: { payload: unknown }) => void>();
  const storage = { getItem: vi.fn(() => null), setItem: vi.fn() };
  const bridge: TauriBridgeApi = {
    invoke,
    isBrowserMode: () => browserMode,
    event: {
      listen: vi.fn((eventName, callback) => {
        listeners.set(eventName, callback);
        return Promise.resolve(() => listeners.delete(eventName));
      }),
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
  vi.stubGlobal("localStorage", storage);
  return {
    controller: new AppController((snapshot) => snapshots.push(snapshot)),
    snapshots,
    listeners,
    storage,
  };
}

describe("AppController settings persistence", () => {
  beforeEach(() => {
    vi.useRealTimers();
  });

  afterEach(() => {
    vi.useRealTimers();
    vi.restoreAllMocks();
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

  it("opens onboarding on the first real Tauri launch", async () => {
    const invoke = vi.fn().mockResolvedValue(undefined);
    const { controller } = createController(invoke, undefined, false);

    await controller.initialize();

    expect(invoke).toHaveBeenCalledWith("open_engine_wizard");
  });

  it("does not auto-open onboarding in browser mode", async () => {
    const invoke = vi.fn().mockResolvedValue(undefined);
    const { controller } = createController(invoke);

    await controller.initialize();

    expect(invoke).not.toHaveBeenCalledWith("open_engine_wizard");
  });

  it("marks onboarding complete only after a successful wizard close", async () => {
    const invoke = vi.fn().mockResolvedValue(undefined);
    const { controller, listeners, storage } = createController(invoke, undefined, false);

    await controller.initialize();
    listeners.get("engine-wizard-closed")?.({ payload: { modelDownloaded: false } });
    expect(storage.setItem).not.toHaveBeenCalled();

    listeners.get("engine-wizard-closed")?.({ payload: { modelDownloaded: true } });
    expect(storage.setItem).toHaveBeenCalledWith("meowcal.onboardingComplete", "true");
  });

  it("uses current settings and a curated source for settings test translation", async () => {
    const invoke = vi.fn().mockResolvedValue({ translatedText: "sample" });
    const { controller } = createController(invoke);
    vi.spyOn(Math, "random").mockReturnValue(0);

    await controller.setLanguage("source", "ja-JP");
    await controller.setLanguage("target", "fr-FR");
    await controller.testTranslation();

    expect(invoke).toHaveBeenCalledWith("wizard_test_translation", {
      sourceText: "時計塔の話は後だ、まずドアを閉めろ。",
      sourceLanguage: "ja-JP",
      targetLanguage: "fr-FR",
    });
  });
});
