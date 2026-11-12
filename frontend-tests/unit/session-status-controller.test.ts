import { afterEach, describe, expect, it, vi } from "vitest";
import { SessionStatusController } from "../../src/ui/session-status-controller";
import type { UiSnapshot } from "../../src/ui/contracts";

function harness() {
  const listeners = new Map<string, (event: { payload: unknown }) => void>();
  let snapshot = { running: true, busy: "idle", error: null } as UiSnapshot;
  const publish = vi.fn((patch: Partial<UiSnapshot>) => {
    snapshot = { ...snapshot, ...patch };
  });
  vi.stubGlobal("window", {
    TauriBridge: {
      event: {
        listen: async (name: string, listener: (event: { payload: unknown }) => void) => {
          listeners.set(name, listener);
          return () => listeners.delete(name);
        },
      },
    },
  });
  return {
    controller: new SessionStatusController(publish, () => snapshot),
    publish,
    current: () => snapshot,
    patch: (patch: Partial<UiSnapshot>) => publish(patch),
    emit: (name: string, payload: unknown) => listeners.get(name)?.({ payload }),
    activity: (sessionId: number, captureId: number, state = "active") =>
      listeners.get("subtitle-activity")?.({ payload: { sessionId, captureId, state } }),
  };
}

afterEach(() => vi.unstubAllGlobals());

describe("session status reports", () => {
  it("keeps healthy activity private without changing Home", async () => {
    const app = harness();
    await app.controller.listen();
    app.activity(1, 60, "waiting");
    expect(app.current()).toMatchObject({
      running: true,
      busy: "idle",
      error: null,
    });
    app.activity(1, 61);
    expect(app.publish).not.toHaveBeenCalled();
  });

  it("rejects older sessions, repeated reports and older capture results", async () => {
    const app = harness();
    await app.controller.listen();
    app.activity(3, 5);
    app.emit("capture-status", { isError: true, message: "Capture unavailable" });
    app.publish.mockClear();
    app.activity(1, 100);
    app.activity(3, 4);
    app.activity(3, 5);
    app.activity(3, 4, "waiting");
    expect(app.publish).not.toHaveBeenCalled();
    expect(app.current().error).toBe("Capture unavailable");
    app.activity(3, 6, "waiting");
    app.activity(3, 6);
    expect(app.publish).toHaveBeenCalledTimes(1);
    app.activity(3, 7);
    expect(app.publish).toHaveBeenCalledTimes(1);
    expect(app.current().error).toBeNull();
  });

  it("retires the old session across stop and a new selection", async () => {
    const app = harness();
    await app.controller.listen();
    app.activity(1, 1);
    app.publish.mockClear();
    app.controller.reset();
    expect(app.publish).not.toHaveBeenCalled();
    app.patch({ running: false, busy: "selecting" });
    app.emit("capture-status", { isError: true, message: "Capture unavailable" });
    app.activity(1, 80);
    app.activity(3, 0);
    expect(app.current().error).toBe("Capture unavailable");
    app.patch({ busy: "starting" });
    app.activity(1, 81);
    expect(app.current().error).toBe("Capture unavailable");
    app.activity(3, 0);
    expect(app.current().error).toBeNull();
    app.patch({ running: true, busy: "idle" });
    app.emit("capture-status", { isError: true, message: "Capture unavailable" });
    app.activity(3, 60, "waiting");
    app.activity(1, 100);
    expect(app.current().error).toBeNull();
    app.activity(3, 61);
    expect(app.current().error).toBeNull();
  });

  it.each([
    null,
    "waiting",
    {},
    { sessionId: -1, captureId: 0, state: "active" },
    { sessionId: 1, captureId: -1, state: "active" },
    { sessionId: 1, captureId: 0.5, state: "waiting" },
    { sessionId: 1, captureId: 1, state: "failed" },
  ])("ignores malformed activity %j", async (payload) => {
    const app = harness();
    await app.controller.listen();
    app.emit("subtitle-activity", payload);
    expect(app.publish).not.toHaveBeenCalled();
  });

  it("clears only owned capture errors after healthy OCR, including a continuing wait", async () => {
    const app = harness();
    await app.controller.listen();
    app.emit("capture-status", { isError: true, message: "Capture unavailable" });
    app.activity(1, 60, "waiting");
    expect(app.current().error).toBeNull();
    app.activity(1, 61);
    expect(app.current().error).toBeNull();
    app.emit("capture-status", { isError: true });
    expect(app.current().error).toBe("Screen capture failed");
    app.patch({ error: "Settings could not be saved" });
    app.activity(1, 62);
    expect(app.current().error).toBe("Settings could not be saved");
  });

  it("preserves compatibility capture warnings across healthy activity", async () => {
    const app = harness();
    await app.controller.listen();
    app.emit("capture-status", null);
    app.emit("capture-status", {});
    app.emit("capture-status", { isError: false, message: "Using GDI fallback" });
    app.activity(1, 1);
    expect(app.current().captureWarning).toBe("Using GDI fallback");
  });

  it("returns to Home after an explicit exit and removes subscriptions on disposal", async () => {
    const app = harness();
    const unlisten = await app.controller.listen();
    app.activity(1, 60, "waiting");
    app.emit("translation-exited", null);
    expect(app.current()).toMatchObject({
      screen: "home",
      running: false,
      busy: "idle",
      error: null,
      notice: "Translation stopped",
    });
    unlisten.forEach((stop) => stop());
    app.publish.mockClear();
    app.activity(3, 1);
    expect(app.publish).not.toHaveBeenCalled();
  });

  it("preserves the actionable OCR startup reason when its session terminates", async () => {
    const app = harness();
    await app.controller.listen();
    app.patch({ running: false, busy: "starting" });
    app.activity(3, 0);
    app.emit("capture-status", {
      isError: true,
      message: "Switch to the Compatibility profile to use Windows OCR.",
    });
    app.emit("translation-interrupted", { sessionId: 3 });
    expect(app.current()).toMatchObject({
      running: false,
      busy: "idle",
      error: "Switch to the Compatibility profile to use Windows OCR.",
    });
    app.publish.mockClear();
    app.emit("translation-interrupted", { sessionId: 3 });
    app.emit("translation-interrupted", { sessionId: 1 });
    app.emit("translation-interrupted", null);
    expect(app.publish).not.toHaveBeenCalled();
  });

  it("reports an unexpected exit even without a preceding capture error", async () => {
    const app = harness();
    await app.controller.listen();
    app.activity(3, 0);
    app.emit("translation-interrupted", { sessionId: 1 });
    expect(app.current().running).toBe(true);
    app.emit("translation-interrupted", { sessionId: 3 });
    expect(app.current()).toMatchObject({
      running: false,
      error: "Translation stopped unexpectedly. Select the subtitle area to start again.",
    });
  });

  it("keeps a terminal session retired when its first activity report was never received", async () => {
    const app = harness();
    await app.controller.listen();
    app.emit("translation-interrupted", { sessionId: 7 });
    app.controller.reset();
    app.patch({ busy: "starting", error: null });
    app.emit("capture-status", { isError: true, message: "Capture unavailable" });
    app.publish.mockClear();
    app.activity(7, 0);
    app.emit("translation-interrupted", { sessionId: 7 });
    expect(app.publish).not.toHaveBeenCalled();
    expect(app.current().error).toBe("Capture unavailable");
    app.activity(9, 0);
    expect(app.current()).toMatchObject({
      busy: "starting",
      error: null,
    });
  });

  it.each([null, "Switch to the Compatibility profile to use Windows OCR."])(
    "keeps the terminal recovery reason on Exit to Home: %s",
    async (reason) => {
      const app = harness();
      await app.controller.listen();
      if (reason) app.emit("capture-status", { isError: true, message: reason });
      app.emit("translation-interrupted", { sessionId: 3 });
      app.emit("translation-exited", null);
      expect(app.current()).toMatchObject({
        screen: "home",
        running: false,
        busy: "idle",
        error:
          reason ?? "Translation stopped unexpectedly. Select the subtitle area to start again.",
        notice: null,
      });
    },
  );

  it("clears capture errors on a normal running Exit and does not retain unrelated errors", async () => {
    const app = harness();
    await app.controller.listen();
    app.emit("capture-status", { isError: true, message: "Capture unavailable" });
    app.emit("translation-exited", null);
    expect(app.current().error).toBeNull();
    app.emit("translation-interrupted", { sessionId: 3 });
    app.patch({ error: "Unrelated settings error" });
    app.emit("translation-exited", null);
    expect(app.current().error).toBeNull();
  });
});
