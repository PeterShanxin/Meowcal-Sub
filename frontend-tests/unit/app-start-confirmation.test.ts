import { afterEach, describe, expect, it, vi } from "vitest";
import { AppController } from "../../src/ui/app-controller";
import { deriveHomePresentation, isSessionLocked } from "../../src/ui/home-state";
import type { CaptureRegion, TauriBridgeApi } from "../../src/ui/contracts";

const saved = { x: 10, y: 800, width: 1200, height: 120 };

type Outcome = { requestId: string; region: CaptureRegion | null; startTranslation: boolean };

async function fixture(region: CaptureRegion | null = saved) {
  let stopFailure = false;
  let startFailure = false;
  let resolve!: (result: Outcome) => void;
  let reject!: (error: Error) => void;
  let requestId = "";
  const listeners = new Map<string, (event: { payload: unknown }) => void>();
  const invoke = vi.fn(async (command: string, args?: Record<string, unknown>) => {
    if (command === "get_capture_region") return region;
    if (command === "stop_translation" && stopFailure) throw new Error("Could not stop capture");
    if (command === "start_translation" && startFailure) throw new Error("Display changed");
    if (command === "get_settings") return {};
    if (command === "get_ocr_languages") return ["zh-CN"];
    if (command === "get_engine_status" || command === "refresh_engine_status")
      return { phase: "ready" };
    if (command === "request_area_selection") {
      requestId = args?.requestId as string;
      return new Promise<Outcome>((accept, fail) => {
        resolve = accept;
        reject = fail;
      });
    }
    return undefined;
  });
  vi.stubGlobal("localStorage", { getItem: () => "true", setItem: vi.fn() });
  vi.stubGlobal("window", {
    TauriBridge: {
      invoke: invoke as TauriBridgeApi["invoke"],
      isBrowserMode: () => true,
      event: {
        listen: async (name: string, listener: (event: { payload: unknown }) => void) => {
          listeners.set(name, listener);
          return () => listeners.delete(name);
        },
      },
    },
    OcrLanguageTags: { isOcrLanguageAvailable: () => true },
    setTimeout,
    clearTimeout,
  });
  const controller = new AppController(() => {});
  await controller.initialize();
  return {
    controller,
    invoke,
    failStop: () => {
      stopFailure = true;
    },
    failStart: () => {
      startFailure = true;
    },
    listeners,
    confirm: (value: CaptureRegion | null = saved, startTranslation = true) =>
      resolve({ requestId, region: value, startTranslation }),
    mismatch: () => resolve({ requestId: "obsolete", region: saved, startTranslation: true }),
    fail: (message: string) => reject(new Error(message)),
  };
}

afterEach(() => {
  vi.useRealTimers();
  vi.unstubAllGlobals();
});

describe("session area confirmation", () => {
  it("does not open a selector while an existing-area start is warming", async () => {
    const { controller, invoke } = await fixture();
    let finishSave!: () => void;
    const saving = new Promise<undefined>((resolve) => {
      finishSave = () => resolve(undefined);
    });
    invoke.mockImplementationOnce(async () => saving);
    const starting = controller.start(true);
    expect(controller.current().busy).toBe("warming");
    await controller.selectRegion();
    expect(invoke).not.toHaveBeenCalledWith("request_area_selection", expect.anything());
    finishSave();
    await starting;
    controller.dispose();
  });
  it("starts with the saved area without opening the selector", async () => {
    const { controller, invoke } = await fixture();
    await controller.start(true);
    expect(invoke).not.toHaveBeenCalledWith("request_area_selection", expect.anything());
    expect(invoke).toHaveBeenCalledWith("start_translation");
    expect(controller.current()).toMatchObject({ running: true, region: saved });
    controller.dispose();
  });

  it("does not start with a missing saved area", async () => {
    const { controller, invoke } = await fixture(null);
    await controller.start(true);
    expect(invoke).not.toHaveBeenCalledWith("start_translation");
    expect(controller.current()).toMatchObject({ running: false, busy: "idle" });
    controller.dispose();
  });

  it("surfaces backend rejection of a stale saved area", async () => {
    const { controller, failStart } = await fixture();
    failStart();
    await controller.start(true);
    expect(controller.current()).toMatchObject({
      running: false,
      busy: "idle",
      error: "Display changed",
    });
    controller.dispose();
  });

  it.each([saved, null])(
    "opens the selector before a session, with cached region %j",
    async (region) => {
      const { controller, invoke, confirm } = await fixture(region);
      const starting = controller.start();
      expect(controller.current()).toMatchObject({ busy: "selecting", running: false });
      expect(isSessionLocked(controller.current())).toBe(true);
      expect(deriveHomePresentation(controller.current()).actionDisabled).toBe(true);
      expect(invoke).toHaveBeenCalledWith(
        "request_area_selection",
        expect.objectContaining({ startTranslation: true }),
      );
      expect(invoke).not.toHaveBeenCalledWith("save_settings", expect.anything());
      confirm();
      await starting;
      expect(controller.current()).toMatchObject({ busy: "idle", running: true, region: saved });
      expect(invoke.mock.calls.filter(([command]) => command === "start_translation")).toHaveLength(
        1,
      );
      controller.dispose();
    },
  );

  it("does not start from coordinate changes, stray events, or elapsed selection time", async () => {
    vi.useFakeTimers();
    const { controller, invoke, confirm, listeners } = await fixture();
    const starting = controller.start();
    listeners.get("region-selected")?.({ payload: { ...saved, y: 760 } });
    await controller.refresh();
    await vi.advanceTimersByTimeAsync(30_000);
    expect(invoke).not.toHaveBeenCalledWith("start_translation");
    expect(controller.current().busy).toBe("selecting");
    confirm();
    await starting;
    expect(controller.current().running).toBe(true);
    controller.dispose();
  });

  it("cancels without replacing the previous crop or starting preparation", async () => {
    const { controller, invoke, confirm } = await fixture();
    const starting = controller.start();
    confirm(null);
    await starting;
    expect(controller.current()).toMatchObject({
      busy: "idle",
      running: false,
      region: saved,
      error: null,
    });
    expect(invoke).not.toHaveBeenCalledWith("save_settings", expect.anything());
    expect(invoke).not.toHaveBeenCalledWith("make_engine_ready");
    controller.dispose();
  });

  it("saves the selected crop without starting a session", async () => {
    const { controller, invoke, confirm } = await fixture();
    const selected = { ...saved, y: 760 };
    const selecting = controller.start();
    confirm(selected, false);
    await selecting;
    expect(controller.current()).toMatchObject({ region: selected, busy: "idle", running: false });
    expect(invoke).not.toHaveBeenCalledWith("start_translation");
    controller.dispose();
  });

  it("accepts an adjusted crop and requires fresh confirmation after Stop", async () => {
    const { controller, invoke, confirm } = await fixture();
    const adjusted = { ...saved, y: 760 };
    const first = controller.start();
    confirm(adjusted);
    await first;
    expect(controller.current().region).toEqual(adjusted);
    await controller.stop();
    const second = controller.start();
    expect(controller.current()).toMatchObject({ running: false, busy: "selecting" });
    const requests = invoke.mock.calls.filter(([command]) => command === "request_area_selection");
    expect(requests).toHaveLength(2);
    expect(requests[0][1]?.requestId).not.toBe(requests[1][1]?.requestId);
    confirm(adjusted);
    await second;
    expect(invoke.mock.calls.filter(([command]) => command === "start_translation")).toHaveLength(
      2,
    );
    controller.dispose();
  });

  it("coalesces repeated Start presses into one confirmation and one session", async () => {
    const { controller, invoke, confirm } = await fixture();
    const first = controller.start();
    await controller.start();
    expect(
      invoke.mock.calls.filter(([command]) => command === "request_area_selection"),
    ).toHaveLength(1);
    confirm();
    await first;
    await controller.start();
    expect(invoke.mock.calls.filter(([command]) => command === "start_translation")).toHaveLength(
      1,
    );
    controller.dispose();
  });

  it.each(["open failed", "mismatched result"])(
    "recovers from %s without starting",
    async (failure) => {
      const { controller, invoke, confirm, fail, mismatch } = await fixture();
      const first = controller.start();
      if (failure === "open failed") fail("Selector unavailable");
      else mismatch();
      await first;
      expect(controller.current()).toMatchObject({ running: false, busy: "idle", region: saved });
      expect(controller.current().error).toBeTruthy();
      expect(invoke).not.toHaveBeenCalledWith("start_translation");
      const retry = controller.start();
      confirm();
      await retry;
      expect(controller.current()).toMatchObject({ running: true, error: null });
      controller.dispose();
    },
  );

  it("cancels a pending request on disposal and ignores its later confirmation", async () => {
    const { controller, invoke, confirm } = await fixture();
    const starting = controller.start();
    const request = invoke.mock.calls.find(([command]) => command === "request_area_selection");
    controller.dispose();
    expect(invoke).toHaveBeenCalledWith("complete_area_selection", {
      requestId: request?.[1]?.requestId,
      region: null,
    });
    confirm();
    await starting;
    expect(invoke).not.toHaveBeenCalledWith("start_translation");
  });
  it.each(["confirm", "cancel", "stop failure"])(
    "checks the area during a running session: %s",
    async (outcome) => {
      const { controller, invoke, confirm, failStop } = await fixture();
      const first = controller.start();
      confirm();
      await first;
      if (outcome === "stop failure") failStop();
      const reselecting = controller.selectRegion();
      if (outcome === "stop failure") {
        await reselecting;
        expect(controller.current()).toMatchObject({
          running: true,
          error: "Could not stop capture",
        });
        expect(
          invoke.mock.calls.filter(([command]) => command === "request_area_selection"),
        ).toHaveLength(1);
      } else {
        await vi.waitFor(() =>
          expect(
            invoke.mock.calls.filter(([command]) => command === "request_area_selection"),
          ).toHaveLength(2),
        );
        expect(controller.current()).toMatchObject({ running: false, busy: "selecting" });
        confirm(outcome === "cancel" ? null : saved);
        await reselecting;
        expect(controller.current()).toMatchObject({
          running: outcome === "confirm",
          busy: "idle",
          region: saved,
        });
        expect(
          invoke.mock.calls.filter(([command]) => command === "start_translation"),
        ).toHaveLength(outcome === "confirm" ? 2 : 1);
      }
      controller.dispose();
    },
  );
  it("keeps capture error recovery active when area selection cannot stop the session", async () => {
    const { controller, confirm, failStop, listeners } = await fixture();
    const starting = controller.start();
    confirm();
    await starting;
    listeners.get("subtitle-activity")?.({
      payload: { sessionId: 2, captureId: 0, state: "active" },
    });
    failStop();
    await controller.selectRegion();
    listeners.get("capture-status")?.({
      payload: { isError: true, message: "Capture unavailable" },
    });
    listeners.get("subtitle-activity")?.({
      payload: { sessionId: 2, captureId: 1, state: "active" },
    });
    expect(controller.current()).toMatchObject({
      running: true,
      error: null,
    });
    controller.dispose();
  });
});
