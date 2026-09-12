import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { AppController } from "../../src/ui/app-controller";
import type { TauriBridgeApi, UiSnapshot } from "../../src/ui/contracts";

function createController(
  invoke: TauriBridgeApi["invoke"],
  emit: TauriBridgeApi["event"]["emit"] = vi.fn().mockResolvedValue(undefined),
  browserMode = true,
  updates?: TauriBridgeApi["updates"],
  clock?: () => number,
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
    updates,
  };
  vi.stubGlobal("window", {
    TauriBridge: bridge,
    setTimeout: ((...args: Parameters<typeof setTimeout>) =>
      globalThis.setTimeout(...args)) as unknown as Window["setTimeout"],
    clearTimeout: ((id: Parameters<typeof clearTimeout>[0]) =>
      globalThis.clearTimeout(id)) as unknown as Window["clearTimeout"],
    setInterval: ((...args: Parameters<typeof setInterval>) =>
      globalThis.setInterval(...args)) as unknown as Window["setInterval"],
    clearInterval: ((id: Parameters<typeof clearInterval>[0]) =>
      globalThis.clearInterval(id)) as unknown as Window["clearInterval"],
  });
  vi.stubGlobal("localStorage", storage);
  return {
    controller: new AppController((snapshot) => snapshots.push(snapshot), clock),
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
      "translate all OCR text",
      (controller: AppController) => controller.setTranslateAllOcrText(true),
    ],
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

  it("defaults to the subtitle-aware gate and persists a change to it", async () => {
    const invoke = vi.fn().mockResolvedValue(undefined);
    const { controller, snapshots } = createController(invoke);

    expect(controller.current().settings.translation.translateAllOcrText).toBe(false);

    await controller.setTranslateAllOcrText(true);

    expect(snapshots.at(-1)?.settings.translation.translateAllOcrText).toBe(true);
    expect(invoke).toHaveBeenCalledWith("save_settings", {
      settings: expect.objectContaining({
        translation: expect.objectContaining({ translateAllOcrText: true }),
      }),
    });
  });

  // Settings written before the toggle existed carry no key for it, and an
  // upgrade must not turn general-text translation on for them.
  it("reads a stored settings object without the toggle as off", async () => {
    const invoke = vi.fn(async (command: string) =>
      command === "get_settings" ? { translation: { enableContextAware: true } } : undefined,
    );
    const { controller, snapshots } = createController(invoke as TauriBridgeApi["invoke"]);

    await controller.initialize();

    expect(snapshots.at(-1)?.settings.translation.translateAllOcrText).toBe(false);
    expect(snapshots.at(-1)?.settings.translation.enableContextAware).toBe(true);
    controller.dispose();
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

  it("updates a preparing engine when background startup finishes", async () => {
    let ready!: (value: unknown) => void;
    const pending = new Promise((resolve) => {
      ready = resolve;
    });
    const invoke = vi.fn(async (command: string) => {
      if (command === "get_engine_status") return { phase: "preparing" };
      if (command === "make_engine_ready") return pending;
      return undefined;
    });
    const { controller } = createController(invoke as TauriBridgeApi["invoke"], undefined, false);
    const initialized = controller.initialize();
    await vi.waitFor(() => expect(invoke).toHaveBeenCalledWith("make_engine_ready"));
    expect(controller.current().engine?.phase).toBe("preparing");
    ready({ phase: "ready" });
    await initialized;
    expect(controller.current().engine?.phase).toBe("ready");
    controller.dispose();
  });

  it("reports a real background startup failure instead of staying preparing", async () => {
    const invoke = vi.fn(async (command: string) => {
      if (command === "get_engine_status") return { phase: "preparing" };
      if (command === "make_engine_ready") throw new Error("Core readiness failed");
      return undefined;
    });
    const { controller } = createController(invoke as TauriBridgeApi["invoke"], undefined, false);
    await controller.initialize();
    expect(controller.current()).toMatchObject({
      engine: { phase: "error" },
      error: "Core readiness failed",
    });
    controller.dispose();
  });

  it("ignores a readiness response after the controller is disposed", async () => {
    let ready!: (value: unknown) => void;
    const pending = new Promise((resolve) => {
      ready = resolve;
    });
    const invoke = vi.fn(async (command: string) => {
      if (command === "get_engine_status") return { phase: "preparing" };
      if (command === "make_engine_ready") return pending;
      return undefined;
    });
    const { controller, snapshots } = createController(
      invoke as TauriBridgeApi["invoke"],
      undefined,
      false,
    );
    const initialized = controller.initialize();
    await vi.waitFor(() => expect(invoke).toHaveBeenCalledWith("make_engine_ready"));
    controller.dispose();
    const count = snapshots.length;
    ready({ phase: "ready" });
    await initialized;
    expect(snapshots).toHaveLength(count);
  });

  // A cancelled setup used to leave the flag unset, so the wizard reopened on
  // every launch and Home's own setup action was never reachable (#74).
  it("stops opening setup by itself once the user has closed it, finished or not", async () => {
    const invoke = vi.fn().mockResolvedValue(undefined);
    const { controller, listeners, storage } = createController(invoke, undefined, false);

    await controller.initialize();
    listeners.get("engine-wizard-closed")?.({ payload: { modelDownloaded: false } });

    expect(storage.setItem).toHaveBeenCalledWith("meowcal.onboardingComplete", "true");
    controller.dispose();
  });

  it("does not reopen setup on a launch after it was closed", async () => {
    const invoke = vi.fn().mockResolvedValue(undefined);
    const { controller, storage } = createController(invoke, undefined, false);
    storage.getItem.mockReturnValue("true" as unknown as null);

    await controller.initialize();

    expect(invoke).not.toHaveBeenCalledWith("open_engine_wizard");
    controller.dispose();
  });

  it("opens area selection when setup hands over to it", async () => {
    const invoke = vi.fn().mockResolvedValue(undefined);
    const { controller, listeners } = createController(invoke);

    await controller.initialize();
    listeners.get("setup-select-area")?.({ payload: null });

    await vi.waitFor(() => expect(invoke).toHaveBeenCalledWith("open_area_selector"));
    controller.dispose();
  });

  it("clears a notice after a few seconds but keeps an error until it is dismissed", async () => {
    vi.useFakeTimers();
    const invoke = vi.fn().mockResolvedValue(undefined);
    const { controller } = createController(invoke);

    await controller.stop();
    expect(controller.current().notice).toBe("Translation stopped");
    await vi.advanceTimersByTimeAsync(4000);
    expect(controller.current().notice).toBeNull();

    invoke.mockRejectedValueOnce(new Error("stop failed"));
    await controller.stop();
    await vi.advanceTimersByTimeAsync(60_000);
    expect(controller.current().error).toBe("stop failed");

    controller.dismissMessage();
    expect(controller.current()).toMatchObject({ error: null, notice: null });
    controller.dispose();
  });

  it("does not let an earlier notice's timer clear a newer notice early", async () => {
    vi.useFakeTimers();
    const invoke = vi.fn().mockResolvedValue(undefined);
    const { controller } = createController(invoke);

    await controller.stop();
    await vi.advanceTimersByTimeAsync(3000);
    await controller.saveSettings();
    await vi.advanceTimersByTimeAsync(3000);

    expect(controller.current().notice).toBe("Settings saved");
    controller.dispose();
  });

  it("takes appearance saved by the overlay menu back when the window regains focus", async () => {
    const invoke = vi.fn(async (command: string) =>
      command === "get_settings" ? { overlay: { fontSize: 36, lightBackground: true } } : undefined,
    );
    const { controller } = createController(invoke as TauriBridgeApi["invoke"]);

    await controller.refresh();

    expect(controller.current().settings.overlay).toMatchObject({
      fontSize: 36,
      lightBackground: true,
    });
  });

  it("keeps an unsaved appearance edit from this window over the stored one", async () => {
    vi.useFakeTimers();
    const invoke = vi.fn(async (command: string) =>
      command === "get_settings" ? { overlay: { fontSize: 36 } } : undefined,
    );
    const { controller } = createController(invoke as TauriBridgeApi["invoke"]);

    await controller.updateOverlay({ fontSize: 40 });
    await controller.refresh();

    expect(controller.current().settings.overlay.fontSize).toBe(40);
    controller.dispose();
  });

  // Diagnostics could be switched on from the overlay's own menu before they
  // moved under Developer options, and they show raw recognition text.
  it("turns persisted overlay diagnostics off at startup outside developer mode", async () => {
    const emit = vi.fn().mockResolvedValue(undefined);
    const invoke = vi.fn(async (command: string) =>
      command === "get_settings" ? { overlay: { showDiagnostics: true } } : undefined,
    );
    const { controller } = createController(invoke as TauriBridgeApi["invoke"], emit);

    await controller.initialize();

    expect(controller.current().settings.overlay.showDiagnostics).toBe(false);
    expect(emit).toHaveBeenCalledWith(
      "overlay-settings-updated",
      expect.objectContaining({ showDiagnostics: false }),
    );
    controller.dispose();
  });

  it("keeps persisted overlay diagnostics in developer mode", async () => {
    const emit = vi.fn().mockResolvedValue(undefined);
    const invoke = vi.fn(async (command: string) =>
      command === "get_settings" ? { overlay: { showDiagnostics: true } } : undefined,
    );
    const { controller } = createController(invoke as TauriBridgeApi["invoke"], emit);
    controller.setDeveloperMode(true);

    await controller.initialize();

    expect(controller.current().settings.overlay.showDiagnostics).toBe(true);
    expect(emit).not.toHaveBeenCalled();
    controller.dispose();
  });

  it("turns overlay diagnostics off with developer mode", async () => {
    const emit = vi.fn().mockResolvedValue(undefined);
    const invoke = vi.fn().mockResolvedValue(undefined);
    const { controller } = createController(invoke, emit);

    await controller.updateOverlay({ showDiagnostics: true });
    controller.setDeveloperMode(false);

    expect(emit).toHaveBeenLastCalledWith(
      "overlay-settings-updated",
      expect.objectContaining({ showDiagnostics: false }),
    );
    controller.dispose();
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

describe("AppController automatic update checks", () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });

  afterEach(() => {
    vi.useRealTimers();
    vi.restoreAllMocks();
    vi.unstubAllGlobals();
  });

  it("schedules an automatic update check 2 seconds after real desktop launch", async () => {
    const check = vi.fn().mockResolvedValue(null);
    const invoke = vi.fn().mockResolvedValue(undefined);
    const updates = {
      currentVersion: vi.fn().mockResolvedValue("0.6.9"),
      check,
      restart: vi.fn(),
    };
    const now = 1_700_000_000_000;
    const { controller, snapshots, storage } = createController(
      invoke,
      undefined,
      false,
      updates,
      () => now,
    );
    storage.getItem.mockReturnValue("true"); // onboarding already completed

    await controller.initialize();

    // UI initialization finishes immediately without waiting for check
    expect(snapshots.at(-1)?.busy).toBe("idle");
    expect(check).not.toHaveBeenCalled();

    // Advancing past 2000ms triggers the check
    await vi.advanceTimersByTimeAsync(2000);

    expect(check).toHaveBeenCalledTimes(1);
    expect(invoke).toHaveBeenCalledWith("save_settings", {
      settings: expect.objectContaining({
        lastUpdateCheckTimeMs: now,
      }),
    });
  });

  it("does not check automatically if autoCheckUpdates is false", async () => {
    const check = vi.fn().mockResolvedValue(null);
    const invoke: TauriBridgeApi["invoke"] = vi.fn(async (cmd) => {
      if (cmd === "get_settings") return { autoCheckUpdates: false } as never;
      return undefined as never;
    });
    const updates = {
      currentVersion: vi.fn().mockResolvedValue("0.6.9"),
      check,
      restart: vi.fn(),
    };
    const { controller, storage } = createController(invoke, undefined, false, updates);
    storage.getItem.mockReturnValue("true");

    await controller.initialize();
    await vi.advanceTimersByTimeAsync(3000);

    expect(check).not.toHaveBeenCalled();
  });

  it("does not check automatically if last check was less than 24h ago", async () => {
    const check = vi.fn().mockResolvedValue(null);
    const now = 1_700_000_000_000;
    const invoke: TauriBridgeApi["invoke"] = vi.fn(async (cmd) => {
      if (cmd === "get_settings") return { lastUpdateCheckTimeMs: now - 3600 * 1000 } as never;
      return undefined as never;
    });
    const updates = {
      currentVersion: vi.fn().mockResolvedValue("0.6.9"),
      check,
      restart: vi.fn(),
    };
    const { controller, storage } = createController(invoke, undefined, false, updates, () => now);
    storage.getItem.mockReturnValue("true");

    await controller.initialize();
    await vi.advanceTimersByTimeAsync(3000);

    expect(check).not.toHaveBeenCalled();
  });

  it("checks automatically after 24h has elapsed", async () => {
    const check = vi.fn().mockResolvedValue(null);
    const now = 1_700_000_000_000;
    const invoke: TauriBridgeApi["invoke"] = vi.fn(async (cmd) => {
      if (cmd === "get_settings") return { lastUpdateCheckTimeMs: now - 25 * 3600 * 1000 } as never;
      return undefined as never;
    });
    const updates = {
      currentVersion: vi.fn().mockResolvedValue("0.6.9"),
      check,
      restart: vi.fn(),
    };
    const { controller, storage } = createController(invoke, undefined, false, updates, () => now);
    storage.getItem.mockReturnValue("true");

    await controller.initialize();
    await vi.advanceTimersByTimeAsync(2000);

    expect(check).toHaveBeenCalledTimes(1);
  });

  it("allows manual check even when autoCheckUpdates is disabled", async () => {
    const check = vi.fn().mockResolvedValue(null);
    const invoke: TauriBridgeApi["invoke"] = vi.fn(async (cmd) => {
      if (cmd === "get_settings") return { autoCheckUpdates: false } as never;
      return undefined as never;
    });
    const updates = {
      currentVersion: vi.fn().mockResolvedValue("0.6.9"),
      check,
      restart: vi.fn(),
    };
    const { controller } = createController(invoke, undefined, false, updates);

    await controller.initialize();
    await controller.checkForUpdates();

    expect(check).toHaveBeenCalledTimes(1);
  });

  it("skips scheduling automatic check in browser mode", async () => {
    const check = vi.fn().mockResolvedValue(null);
    const invoke = vi.fn().mockResolvedValue(undefined);
    const updates = {
      currentVersion: vi.fn().mockResolvedValue("0.6.9"),
      check,
      restart: vi.fn(),
    };
    const { controller } = createController(invoke, undefined, true, updates);

    await controller.initialize();
    await vi.advanceTimersByTimeAsync(5000);

    expect(check).not.toHaveBeenCalled();
  });

  it("cancels pending automatic check timer on dispose", async () => {
    const check = vi.fn().mockResolvedValue(null);
    const invoke = vi.fn().mockResolvedValue(undefined);
    const updates = {
      currentVersion: vi.fn().mockResolvedValue("0.6.9"),
      check,
      restart: vi.fn(),
    };
    const { controller, storage } = createController(invoke, undefined, false, updates);
    storage.getItem.mockReturnValue("true");

    await controller.initialize();
    controller.dispose();
    await vi.advanceTimersByTimeAsync(5000);

    expect(check).not.toHaveBeenCalled();
  });

  it("updates autoCheckUpdates preference and persists settings", async () => {
    const invoke = vi.fn().mockResolvedValue(undefined);
    const { controller, snapshots } = createController(invoke);

    await controller.updatePreference("autoCheckUpdates", false);

    expect(snapshots.at(-1)?.settings.autoCheckUpdates).toBe(false);
    expect(invoke).toHaveBeenCalledWith("save_settings", {
      settings: expect.objectContaining({
        autoCheckUpdates: false,
      }),
    });
  });

  it("persists timestamp after automatic check when settings loaded successfully", async () => {
    const check = vi.fn().mockResolvedValue(null);
    const now = 1_700_000_000_000;
    const invoke: TauriBridgeApi["invoke"] = vi.fn(async (cmd) => {
      if (cmd === "get_settings") {
        return { sourceLanguage: "ja-JP", lastUpdateCheckTimeMs: null } as never;
      }
      return undefined as never;
    });
    const updates = {
      currentVersion: vi.fn().mockResolvedValue("0.6.9"),
      check,
      restart: vi.fn(),
    };
    const { controller, storage } = createController(invoke, undefined, false, updates, () => now);
    storage.getItem.mockReturnValue("true");

    await controller.initialize();
    await vi.advanceTimersByTimeAsync(2000);

    expect(check).toHaveBeenCalledTimes(1);
    expect(invoke).toHaveBeenCalledWith("save_settings", {
      settings: expect.objectContaining({
        sourceLanguage: "ja-JP",
        lastUpdateCheckTimeMs: now,
      }),
    });
  });

  it("suppresses background persistence on automatic check when get_settings failed", async () => {
    const check = vi.fn().mockResolvedValue(null);
    const now = 1_700_000_000_000;
    const invoke: TauriBridgeApi["invoke"] = vi.fn(async (cmd) => {
      if (cmd === "get_settings") throw new Error("database locked");
      return undefined as never;
    });
    const updates = {
      currentVersion: vi.fn().mockResolvedValue("0.6.9"),
      check,
      restart: vi.fn(),
    };
    const { controller, storage, snapshots } = createController(
      invoke,
      undefined,
      false,
      updates,
      () => now,
    );
    storage.getItem.mockReturnValue("true");

    await controller.initialize();
    await vi.advanceTimersByTimeAsync(2000);

    expect(check).toHaveBeenCalledTimes(1);
    expect(snapshots.at(-1)?.settings.lastUpdateCheckTimeMs).toBe(now);
    expect(invoke).not.toHaveBeenCalledWith("save_settings", expect.anything());
  });
});
