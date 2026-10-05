import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { AppController } from "../../src/ui/app-controller";
import { deriveHomePresentation } from "../../src/ui/home-state";
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

  it("returns to Home with a stopped session when translation is exited from the overlay", async () => {
    const invoke = vi.fn(async (command: string) =>
      ["get_engine_status", "refresh_engine_status", "make_engine_ready"].includes(command)
        ? { phase: "ready" }
        : undefined,
    );
    const { controller, listeners } = createController(invoke as TauriBridgeApi["invoke"]);
    await controller.initialize();
    await controller.start();
    expect(controller.current().running).toBe(true);
    controller.setScreen("settings");

    listeners.get("translation-exited")?.({ payload: null });

    expect(controller.current()).toMatchObject({
      screen: "home",
      running: false,
      busy: "idle",
      captureWarning: null,
      notice: "Translation stopped",
    });
    controller.dispose();
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

    expect(snapshots.at(-1)?.settingsSave).toEqual({
      kind: "error",
      message: "settings unavailable",
    });
    expect(invoke).toHaveBeenCalledWith("save_settings", expect.anything());
  });

  it("defaults the engine to automatic acceleration and persists CPU only", async () => {
    const invoke = vi.fn().mockResolvedValue(undefined);
    const { controller, snapshots } = createController(invoke);

    expect(controller.current().settings.translation.localEngine.cpuOnly).toBe(false);

    await controller.setCpuOnly(true);

    expect(snapshots.at(-1)?.settings.translation.localEngine.cpuOnly).toBe(true);
    expect(invoke).toHaveBeenCalledWith("save_settings", {
      settings: expect.objectContaining({
        translation: expect.objectContaining({
          localEngine: expect.objectContaining({ cpuOnly: true }),
        }),
      }),
    });
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
    expect(controller.current().settingsSave).toEqual({ kind: "saving" });
    await vi.advanceTimersByTimeAsync(250);

    expect(snapshots.at(-1)?.settingsSave).toEqual({
      kind: "error",
      message: "appearance unavailable",
    });
    controller.dispose();
  });

  it("keeps an appearance edit unsaved while its debounced write is pending", async () => {
    vi.useFakeTimers();
    let finishFirst!: () => void;
    const firstWrite = new Promise<void>((resolve) => {
      finishFirst = resolve;
    });
    const invoke = vi.fn().mockReturnValueOnce(firstWrite).mockResolvedValue(undefined);
    const { controller } = createController(invoke);

    const first = controller.setContinuity(true);
    await Promise.resolve();
    await controller.updateOverlay({ fontSize: 40 });
    finishFirst();
    await first;

    expect(controller.current().settingsSave).toEqual({ kind: "saving" });
    expect(invoke).toHaveBeenCalledTimes(1);
    await vi.advanceTimersByTimeAsync(250);
    expect(invoke).toHaveBeenCalledTimes(2);
    expect(invoke.mock.calls.at(-1)?.[1]?.settings.overlay.fontSize).toBe(40);
    expect(controller.current().settingsSave).toEqual({ kind: "idle" });
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

  it.each(["event", "poll"])("prewarms only after area confirmation via %s", async (via) => {
    vi.useFakeTimers();
    let region = { x: 10, y: 800, width: 1200, height: 120 };
    let finish!: (value: unknown) => void;
    const pending = new Promise((resolve) => (finish = resolve));
    const invoke = vi.fn(async (command: string) => {
      if (command === "get_engine_status") return { phase: "notRunning" };
      if (command === "get_capture_region") return region;
      if (command === "is_translation_running") return false;
      if (command === "make_engine_ready") return pending;
      return undefined;
    });
    const { controller, listeners } = createController(
      invoke as TauriBridgeApi["invoke"],
      undefined,
      false,
    );
    await controller.initialize();
    await controller.selectRegion();
    await vi.advanceTimersByTimeAsync(500);
    expect(invoke).not.toHaveBeenCalledWith("make_engine_ready");
    region = { ...region, y: 760 };
    if (via === "event") listeners.get("region-selected")?.({ payload: region });
    else await vi.advanceTimersByTimeAsync(250);
    expect(invoke.mock.calls.filter(([name]) => name === "make_engine_ready")).toHaveLength(1);
    expect(controller.current()).toMatchObject({ busy: "idle", running: false, region });
    expect(invoke).not.toHaveBeenCalledWith("start_translation");
    finish({ phase: "ready" });
    await vi.advanceTimersByTimeAsync(0);
    expect(controller.current().engine?.phase).toBe("ready");
    controller.dispose();
  });

  it.each(["browser", "running", "saving"])("skips area prewarming while %s", async (state) => {
    const invoke = vi.fn(async (command: string) => {
      if (command === "get_engine_status") return { phase: "notRunning" };
      if (command === "is_translation_running") return state === "running";
      if (command === "save_settings") return new Promise(() => {});
      return undefined;
    });
    const { controller, listeners } = createController(
      invoke as TauriBridgeApi["invoke"],
      undefined,
      state === "browser",
    );
    await controller.initialize();
    if (state === "saving") void controller.setCpuOnly(true);
    listeners.get("region-selected")?.({ payload: { x: 0, y: 0, width: 800, height: 100 } });
    expect(invoke).not.toHaveBeenCalledWith("make_engine_ready");
    controller.dispose();
  });

  it.each(["ready", "failed", "immediate start"])(
    "preserves Home Start across selector focus refresh and %s prewarm",
    async (outcome) => {
      vi.useFakeTimers();
      const region = { x: 10, y: 800, width: 1200, height: 120 };
      let finish!: (value: { phase: string }) => void;
      let fail!: (error: Error) => void;
      let phase = "notRunning";
      let preparations = 0;
      const pending = new Promise<{ phase: string }>((resolve, reject) => {
        finish = resolve;
        fail = reject;
      });
      const invoke = vi.fn(async (command: string) => {
        if (command === "get_engine_status" || command === "refresh_engine_status")
          return { phase };
        if (command === "get_capture_region") return region;
        if (command === "is_translation_running") return false;
        if (command === "make_engine_ready") {
          phase = "busy";
          try {
            const result = ++preparations === 1 ? await pending : { phase: "ready" };
            phase = result.phase;
            return result;
          } catch (error) {
            phase = "notRunning";
            throw error;
          }
        }
        return undefined;
      });
      const { controller, listeners } = createController(
        invoke as TauriBridgeApi["invoke"],
        undefined,
        false,
      );
      Object.assign(window, { OcrLanguageTags: { isOcrLanguageAvailable: () => true } });
      try {
        await controller.initialize();
        listeners.get("region-selected")?.({ payload: region });
        await controller.refresh(); // The main window regains focus when the selector closes.
        expect(deriveHomePresentation(controller.current())).toMatchObject({
          action: "start",
          actionDisabled: false,
        });
        const start = outcome === "immediate start" ? controller.start() : null;
        if (outcome === "failed") fail(new Error("Model failed to load"));
        else finish({ phase: "ready" });
        await vi.advanceTimersByTimeAsync(0);
        if (!start) {
          expect(deriveHomePresentation(controller.current())).toMatchObject({
            action: "start",
            actionDisabled: false,
          });
          expect(controller.current().error).toBe(
            outcome === "failed" ? "Model failed to load" : null,
          );
          expect(controller.current().engine?.phase).toBe(
            outcome === "failed" ? "notRunning" : "ready",
          );
        }
        await (start ?? controller.start());
        expect(controller.current()).toMatchObject({ running: true, error: null });
        expect(preparations).toBe(outcome === "failed" ? 2 : 1);
      } finally {
        controller.dispose();
      }
    },
  );

  // Restored regions do not load the engine at launch.
  it("readies a stopped engine before starting translation", async () => {
    const invoke = vi.fn(async (command: string) => {
      if (command === "refresh_engine_status") return { phase: "notRunning" };
      if (command === "make_engine_ready") return { phase: "ready" };
      return undefined;
    });
    const { controller, snapshots } = createController(invoke as TauriBridgeApi["invoke"]);

    await controller.start();

    expect(invoke.mock.calls.map(([command]) => command)).toEqual([
      "save_settings",
      "refresh_engine_status",
      "make_engine_ready",
      "start_translation",
    ]);
    expect(snapshots.at(-1)).toMatchObject({
      busy: "idle",
      engine: { phase: "ready" },
      running: true,
    });
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

  it.each([
    ["browser mode", true, "backendUnavailable"],
    ["Tauri", false, "unknown"],
  ])("marks an unanswered engine status in %s as %s", async (_mode, browser, phase) => {
    const invoke = vi.fn().mockRejectedValue(new Error("Failed to fetch"));
    vi.spyOn(console, "warn").mockImplementation(() => {});
    const { controller } = createController(invoke, undefined, browser);

    await controller.initialize();

    expect(controller.current().engine).toEqual({ phase });
    controller.dispose();
  });

  it.each([
    ["browser mode", true, "backendUnavailable"],
    ["Tauri", false, "ready"],
  ])("handles a lost engine status after startup in %s", async (_mode, browser, phase) => {
    let online = true;
    const invoke = vi.fn(async (command: string) => {
      if (command === "get_engine_status") return { phase: "ready" };
      if (command === "refresh_engine_status") {
        if (!online) throw new Error("Failed to fetch");
        return { phase: "ready" };
      }
      return undefined;
    });
    vi.spyOn(console, "warn").mockImplementation(() => {});
    const { controller } = createController(invoke as TauriBridgeApi["invoke"], undefined, browser);
    await controller.initialize();
    expect(controller.current().engine?.phase).toBe("ready");

    online = false;
    await controller.refresh();
    expect(controller.current().engine?.phase).toBe(phase);

    online = true;
    await controller.refresh();
    expect(controller.current().engine?.phase).toBe("ready");
    controller.dispose();
  });

  it.each([false, true])(
    "coalesces readiness and allows a fresh attempt after failure=%s",
    async (fail) => {
      let finish!: (value: unknown) => void;
      let reject!: (reason: Error) => void;
      let pending = new Promise((resolve, decline) => {
        finish = resolve;
        reject = decline;
      });
      const invoke = vi.fn(async (command: string) => {
        if (command === "refresh_engine_status") return { phase: "preparing" };
        if (command === "make_engine_ready") return pending;
        return undefined;
      });
      const { controller } = createController(invoke as TauriBridgeApi["invoke"], undefined, false);
      const refreshes = Promise.all([
        controller.refresh(),
        controller.refresh(),
        controller.refresh(),
      ]);
      await vi.waitFor(() => expect(invoke).toHaveBeenCalledWith("make_engine_ready"));
      const count = () => invoke.mock.calls.filter(([name]) => name === "make_engine_ready").length;
      const initialCount = count();
      if (fail) reject(new Error("readiness failed"));
      else finish({ phase: "ready" });
      await refreshes;
      expect(initialCount).toBe(1);
      expect(controller.current().engine?.phase).toBe(fail ? "error" : "ready");
      pending = Promise.resolve({ phase: "ready" });
      await controller.refresh();
      expect(count()).toBe(2);
      controller.dispose();
    },
  );

  it.each(["ready", "error", "notRunning"])(
    "rechecks busy until %s without preparation",
    async (phase) => {
      vi.useFakeTimers();
      let result = { phase: "busy" };
      const invoke = vi.fn(async (command: string) =>
        command === "refresh_engine_status" ? result : undefined,
      );
      const { controller } = createController(invoke as TauriBridgeApi["invoke"], undefined, false);
      await Promise.all([controller.refresh(), controller.refresh(), controller.refresh()]);
      expect(controller.current().engine?.phase).toBe("busy");
      result = { phase };
      await vi.advanceTimersByTimeAsync(1000);
      expect(controller.current().engine?.phase).toBe(phase);
      expect(invoke.mock.calls.filter(([name]) => name === "refresh_engine_status")).toHaveLength(
        4,
      );
      expect(invoke).not.toHaveBeenCalledWith("make_engine_ready");
      await vi.advanceTimersByTimeAsync(5000);
      expect(invoke.mock.calls.filter(([name]) => name === "refresh_engine_status")).toHaveLength(
        4,
      );
      controller.dispose();
    },
  );

  it("bounds busy rechecks and cancels the pending timer on dispose", async () => {
    vi.useFakeTimers();
    const invoke = vi.fn(async (command: string) =>
      command === "refresh_engine_status" ? { phase: "busy" } : undefined,
    );
    const { controller } = createController(invoke as TauriBridgeApi["invoke"], undefined, false);
    await controller.refresh();
    await vi.advanceTimersByTimeAsync(130000);
    expect(invoke.mock.calls.filter(([name]) => name === "refresh_engine_status")).toHaveLength(
      121,
    );
    controller.dispose();
    const second = createController(
      invoke as TauriBridgeApi["invoke"],
      undefined,
      false,
    ).controller;
    await second.refresh();
    second.dispose();
    await vi.advanceTimersByTimeAsync(5000);
    expect(invoke.mock.calls.filter(([name]) => name === "refresh_engine_status")).toHaveLength(
      122,
    );
  });

  it.each([false, true])("ignores a stale busy recheck after dispose=%s", async (disposed) => {
    vi.useFakeTimers();
    let finish!: (engine: unknown) => void;
    const pending = new Promise((resolve) => {
      finish = resolve;
    });
    let calls = 0;
    const invoke = vi.fn(async (command: string) => {
      if (command !== "refresh_engine_status") return undefined;
      if (++calls === 1) return { phase: "busy" };
      if (calls === 2) return pending;
      return { phase: "error" };
    });
    const { controller, snapshots } = createController(
      invoke as TauriBridgeApi["invoke"],
      undefined,
      false,
    );
    await controller.refresh();
    await vi.advanceTimersByTimeAsync(1000);
    await vi.advanceTimersByTimeAsync(5000);
    expect(calls).toBe(2);
    if (disposed) controller.dispose();
    else await controller.refresh();
    const published = snapshots.length;
    finish({ phase: "ready" });
    await vi.advanceTimersByTimeAsync(0);
    expect(snapshots).toHaveLength(published);
    expect(controller.current().engine?.phase).toBe(disposed ? "busy" : "error");
    controller.dispose();
  });

  it("reports a failed busy recheck and blocks Start while installation is busy", async () => {
    vi.useFakeTimers();
    let fail = false;
    const invoke = vi.fn(async (command: string) => {
      if (command !== "refresh_engine_status") return undefined;
      if (fail) throw new Error("status unavailable");
      return { phase: "busy" };
    });
    const { controller } = createController(invoke as TauriBridgeApi["invoke"], undefined, false);
    await controller.refresh();
    await controller.start();
    expect(controller.current().running).toBe(false);
    expect(invoke).not.toHaveBeenCalledWith("start_translation");
    expect(invoke).not.toHaveBeenCalledWith("make_engine_ready");
    fail = true;
    await vi.advanceTimersByTimeAsync(1000);
    expect(controller.current()).toMatchObject({
      engine: { phase: "error" },
      error: "status unavailable",
    });
    controller.dispose();
  });

  it("ignores an older refresh that finishes after a newer status", async () => {
    let finish!: (engine: unknown) => void;
    const pending = new Promise((resolve) => {
      finish = resolve;
    });
    let calls = 0;
    const invoke = vi.fn(async (command: string) =>
      command === "refresh_engine_status"
        ? ++calls === 1
          ? pending
          : { phase: "error" }
        : undefined,
    );
    const { controller } = createController(invoke as TauriBridgeApi["invoke"], undefined, false);
    const first = controller.refresh();
    await controller.refresh();
    finish({ phase: "preparing" });
    await first;
    expect(controller.current().engine?.phase).toBe("error");
    expect(invoke).not.toHaveBeenCalledWith("make_engine_ready");
    controller.dispose();
  });

  it.each([false, true])("ignores stale preparation success/failure=%s", async (fail) => {
    let finish!: (engine: unknown) => void;
    let reject!: (error: Error) => void;
    const pending = new Promise((resolve, decline) => {
      finish = resolve;
      reject = decline;
    });
    let phase = "preparing";
    const invoke = vi.fn(async (command: string) => {
      if (command === "refresh_engine_status") return { phase };
      if (command === "make_engine_ready") return pending;
      return undefined;
    });
    const { controller } = createController(invoke as TauriBridgeApi["invoke"], undefined, false);
    const first = controller.refresh();
    await vi.waitFor(() => expect(invoke).toHaveBeenCalledWith("make_engine_ready"));
    phase = "error";
    await controller.refresh();
    if (fail) reject(new Error("old failure"));
    else finish({ phase: "ready" });
    await first;
    expect(controller.current()).toMatchObject({ engine: { phase: "error" }, error: null });
    controller.dispose();
  });

  it("does not prepare an engine whose status is busy", async () => {
    const invoke = vi.fn(async (command: string) =>
      command === "refresh_engine_status" ? { phase: "busy" } : undefined,
    );
    const { controller } = createController(invoke as TauriBridgeApi["invoke"], undefined, false);
    await controller.refresh();
    expect(invoke).not.toHaveBeenCalledWith("make_engine_ready");
    controller.dispose();
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

  it("keeps a capture error reported while away when the window regains focus", async () => {
    const invoke = vi.fn().mockResolvedValue(undefined);
    const { controller, listeners } = createController(invoke);
    await controller.initialize();

    listeners.get("capture-status")?.({
      payload: { isError: true, message: "Capture failed: lost" },
    });
    await controller.refresh();

    expect(controller.current().error).toBe("Capture failed: lost");
    controller.dispose();
  });

  // Fallback reports arrive while the user watches the video, so a timed
  // notice would expire unseen.
  it("keeps a non-fatal capture report for the session until the next start", async () => {
    const invoke = vi.fn(async (command: string) =>
      command === "refresh_engine_status" ? { phase: "ready" } : undefined,
    );
    const { controller, listeners } = createController(invoke as TauriBridgeApi["invoke"]);
    await controller.initialize();

    listeners.get("capture-status")?.({
      payload: { isError: false, usingFallback: true, message: "Using GDI fallback" },
    });
    expect(controller.current()).toMatchObject({
      captureWarning: "Using GDI fallback",
      error: null,
    });

    await controller.start();
    expect(controller.current()).toMatchObject({ captureWarning: null, running: true });
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
    const invoke = vi.fn(async (command: string) =>
      command === "refresh_engine_status" ? { phase: "ready" } : { translatedText: "sample" },
    );
    const { controller } = createController(invoke as TauriBridgeApi["invoke"]);
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

  // Core rejects a completion until the engine is ready, and launch does not load it.
  it("readies a stopped engine before the settings sample translation", async () => {
    const invoke = vi.fn(async (command: string) => {
      if (command === "refresh_engine_status") return { phase: "notRunning" };
      if (command === "make_engine_ready") return { phase: "ready" };
      if (command === "wizard_test_translation") return { translatedText: "sample", latencyMs: 12 };
      return undefined;
    });
    const { controller, snapshots } = createController(invoke as TauriBridgeApi["invoke"]);

    await controller.testTranslation();

    expect(invoke.mock.calls.map(([command]) => command)).toEqual([
      "refresh_engine_status",
      "make_engine_ready",
      "wizard_test_translation",
    ]);
    expect(snapshots.at(-1)).toMatchObject({
      busy: "idle",
      engine: { phase: "ready" },
      notice: "Sample passed · 12 ms",
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

describe("AppController area selection", () => {
  const saved = { x: 10, y: 800, width: 1200, height: 120 };

  beforeEach(() => {
    vi.useFakeTimers();
  });

  afterEach(() => {
    vi.useRealTimers();
    vi.restoreAllMocks();
    vi.unstubAllGlobals();
  });

  // The selector leaves the saved area in place while it is open, so polling
  // must wait for a different area rather than report the saved one as new.
  it("does not confirm a selection while the saved area is unchanged", async () => {
    let region: typeof saved | null = saved;
    const invoke = vi.fn(async (command: string) =>
      command === "get_capture_region" ? region : undefined,
    );
    const { controller } = createController(invoke as TauriBridgeApi["invoke"]);
    await controller.initialize();

    await controller.selectRegion();
    await vi.advanceTimersByTimeAsync(2000);

    expect(controller.current().notice).toBeNull();

    region = { ...saved, y: 760 };
    await vi.advanceTimersByTimeAsync(250);

    expect(controller.current()).toMatchObject({
      region: { y: 760 },
      notice: "Subtitle area selected",
    });
    controller.dispose();
  });

  it("confirms the first area found when none was saved", async () => {
    let region: typeof saved | null = null;
    const invoke = vi.fn(async (command: string) =>
      command === "get_capture_region" ? region : undefined,
    );
    const { controller } = createController(invoke as TauriBridgeApi["invoke"]);
    await controller.initialize();

    await controller.selectRegion();
    region = saved;
    await vi.advanceTimersByTimeAsync(250);

    expect(controller.current()).toMatchObject({
      region: saved,
      notice: "Subtitle area selected",
    });
    controller.dispose();
  });
});

describe("AppController progress messages", () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });

  afterEach(() => {
    vi.useRealTimers();
    vi.restoreAllMocks();
    vi.unstubAllGlobals();
  });

  it("keeps the progress message for a slow sample translation until it finishes", async () => {
    const invoke = vi.fn(async (command: string) => {
      if (command === "refresh_engine_status") return { phase: "ready" };
      if (command === "wizard_test_translation") {
        await new Promise((resolve) => setTimeout(resolve, 9000));
        return { translatedText: "sample" };
      }
      return undefined;
    });
    const { controller } = createController(invoke as TauriBridgeApi["invoke"]);

    const running = controller.testTranslation();
    await vi.advanceTimersByTimeAsync(6000);

    expect(controller.current()).toMatchObject({
      busy: "saving",
      notice: "Running a private sample translation…",
    });

    await vi.advanceTimersByTimeAsync(3000);
    await running;

    expect(controller.current()).toMatchObject({ busy: "idle", notice: "Sample passed" });
    controller.dispose();
  });

  it("drops the progress message when the sample translation fails", async () => {
    const invoke = vi.fn(async (command: string) => {
      if (command === "refresh_engine_status") return { phase: "ready" };
      throw new Error("ENGINE_SAMPLE_TRANSLATION_FAILED");
    });
    const { controller } = createController(invoke as TauriBridgeApi["invoke"]);

    await controller.testTranslation();

    expect(controller.current()).toMatchObject({
      busy: "idle",
      notice: null,
      error: "ENGINE_SAMPLE_TRANSLATION_FAILED",
    });
    controller.dispose();
  });
});

describe("settings write recovery", () => {
  afterEach(() => {
    vi.restoreAllMocks();
    vi.unstubAllGlobals();
  });

  it("keeps a failed edit explicitly unsaved and clears it after retry", async () => {
    const invoke = vi
      .fn()
      .mockRejectedValueOnce(new Error("disk full"))
      .mockResolvedValue(undefined);
    const { controller } = createController(invoke);
    await controller.setContinuity(true);
    expect(controller.current().settingsSave).toEqual({ kind: "error", message: "disk full" });
    expect(controller.current().settings.translation.enableContextAware).toBe(true);
    await controller.saveSettings();
    expect(controller.current().settingsSave).toEqual({ kind: "idle" });
    expect(controller.current().notice).toBe("Settings saved");
    expect(invoke.mock.calls.at(-1)?.[1]?.settings.translation.enableContextAware).toBe(true);
    controller.dispose();
  });

  it("writes rapid edits in order so an older request cannot overwrite the latest settings", async () => {
    let release!: () => void;
    const pending = new Promise<void>((resolve) => {
      release = resolve;
    });
    const invoke = vi.fn().mockReturnValueOnce(pending).mockResolvedValue(undefined);
    const { controller } = createController(invoke);
    const first = controller.setContinuity(true);
    const second = controller.setCpuOnly(true);
    await Promise.resolve();
    expect(invoke).toHaveBeenCalledTimes(1);
    expect(controller.current().settingsSave).toEqual({ kind: "saving" });
    release();
    await Promise.all([first, second]);
    expect(invoke).toHaveBeenCalledTimes(2);
    expect(invoke.mock.calls.at(-1)?.[1]?.settings.translation).toMatchObject({
      enableContextAware: true,
      localEngine: { cpuOnly: true },
    });
    expect(controller.current().settingsSave).toEqual({ kind: "idle" });
    controller.dispose();
  });
});
