import { afterEach, describe, expect, it, vi } from "vitest";

import "../../src/scripts/overlay-session-status.js";
import { setUiLanguage } from "../../src/i18n/index";
const { createSessionStatus, setupSessionStatus } = globalThis.OverlaySessionStatus;
afterEach(() => setUiLanguage("en"));
const payload = (displayState, captureId = 1, warnings = [], sessionId = 1) => ({
  displayState,
  captureId,
  warnings,
  sessionId,
});

describe("overlay session status", () => {
  it.each(["noSubtitleText", "waiting"])(
    "ends recovery preparation when a healthy quiet region reports %s",
    (quietState) => {
      const state = createSessionStatus();
      state.visibility(true);
      state.activity({ sessionId: 1, captureId: 1, state: "active" });
      state.translation(payload("warming", 2));
      expect(state.view()).toEqual({ text: "Preparing translation…", error: false });
      if (quietState === "noSubtitleText") state.translation(payload(quietState, 3));
      else state.activity({ sessionId: 1, captureId: 60, state: quietState });
      expect(state.view()).toEqual(
        quietState === "waiting" ? { text: "Waiting for subtitles…", error: false } : null,
      );
    },
  );

  it("shows preparation, keeps short gaps quiet and reports a prolonged gap", () => {
    const state = createSessionStatus();
    expect(state.view()).toBeNull();
    state.visibility(true);
    expect(state.view()).toEqual({ text: "Preparing translation…", error: false });
    state.translation(payload("translated"));
    for (const [index, kind] of [
      "sourceUnreadable",
      "engineSlow",
      "noSubtitleText",
      "temporarilyUnavailable",
    ].entries()) {
      state.translation(payload(kind, index + 2));
      expect(state.view()).toBeNull();
    }
    state.activity({ sessionId: 1, captureId: 7, state: "waiting" });
    expect(state.view()).toEqual({ text: "Waiting for subtitles…", error: false });
  });

  it("keeps waiting until source subtitles return, then allows a later gap to wait again", () => {
    const state = createSessionStatus();
    state.visibility(true);
    state.activity({ sessionId: 1, captureId: 1, state: "active" });
    state.translation(payload("warming", 2));
    state.activity({ sessionId: 1, captureId: 60, state: "waiting" });
    state.translation(payload("translated", 3));
    state.translation(payload("noSubtitleText", 61));
    expect(state.view()).toEqual({ text: "Waiting for subtitles…", error: false });
    state.capture({ isError: true });
    expect(state.view().error).toBe(true);
    state.activity({ sessionId: 1, captureId: 62, state: "waiting" });
    expect(state.view()).toEqual({ text: "Waiting for subtitles…", error: false });
    state.activity({ sessionId: 1, captureId: 63, state: "active" });
    expect(state.view()).toBeNull();
    state.translation(payload("noSubtitleText", 63));
    expect(state.view()).toBeNull();
    state.activity({ sessionId: 1, captureId: 124, state: "waiting" });
    expect(state.view()).toEqual({ text: "Waiting for subtitles…", error: false });
  });

  it("keeps engine failure until translation succeeds, even across healthy capture and empty frames", () => {
    const state = createSessionStatus();
    state.visibility(true);
    state.translation(
      payload("temporarilyUnavailable", 1, ["local_engine: engine_recovery_failed"]),
    );
    state.activity({ sessionId: 1, captureId: 2, state: "active" });
    state.translation(payload("noSubtitleText", 3));
    expect(state.view()).toEqual({ text: "Translation engine stopped.", error: true });
    state.translation(payload("translated", 4));
    expect(state.view()).toBeNull();
  });

  it("reports capture interruptions separately and clears only after a healthy observation", () => {
    const state = createSessionStatus();
    state.visibility(true);
    state.capture({ isError: true, message: "Private backend details" });
    state.capture({ isError: false });
    expect(state.view()).toEqual({ text: "Translation is interrupted.", error: true });
    state.translation(payload("translated"));
    expect(state.view().error).toBe(true);
    state.activity({ sessionId: 1, captureId: 2, state: "waiting" });
    expect(state.view()).toEqual({ text: "Waiting for subtitles…", error: false });
    state.activity({ sessionId: 1, captureId: 3, state: "active" });
    expect(state.view()).toBeNull();
  });

  it("keeps a terminated session interrupted despite late healthy observations", () => {
    const state = createSessionStatus();
    state.visibility(true);
    state.translation(payload("translated", 1, [], 3));
    state.interrupted({ sessionId: 3 });
    state.activity({ sessionId: 3, captureId: 2, state: "active" });
    state.translation(payload("translated", 3, [], 3));
    state.translation(payload("noSubtitleText", 4, [], 3));
    state.activity({ sessionId: 3, captureId: 5, state: "waiting" });
    expect(state.view()).toEqual({ text: "Translation is interrupted.", error: true });
    state.visibility(false);
    state.visibility(true);
    state.activity({ sessionId: 4, captureId: 1, state: "active" });
    expect(state.view()).toBeNull();
    state.interrupted({ sessionId: 3 });
    expect(state.view()).toBeNull();
  });

  it("accepts a first terminal event while rejecting invalid and stale sessions", () => {
    const state = createSessionStatus();
    state.visibility(true);
    state.translation(payload("translated", 1, [], 3));
    state.interrupted({ sessionId: 2 });
    state.interrupted({});
    expect(state.view()).toBeNull();
    state.interrupted({ sessionId: 4 });
    state.translation(payload("translated", 2, [], 3));
    expect(state.view()).toEqual({ text: "Translation is interrupted.", error: true });
  });

  it("rejects stale sessions and updates within one event stream", () => {
    const state = createSessionStatus();
    state.visibility(true);
    state.translation(payload("translated", 5, [], 3));
    state.translation(payload("warming", 4, [], 3));
    state.translation(payload("warming", 10, [], 2));
    expect(state.view()).toBeNull();
    state.translation(payload("warming", 6, [], 3));
    expect(state.view().error).toBe(false);
    state.activity({ sessionId: 2, captureId: 11, state: "active" });
    expect(state.view()).not.toBeNull();
    state.activity({ sessionId: 3, captureId: 7, state: "active" });
    expect(state.view()).toBeNull();
    state.capture({ isError: true });
    state.activity({ sessionId: 3, captureId: 7, state: "active" });
    state.activity({ sessionId: 3, state: "active" });
    expect(state.view().error).toBe(true);
  });

  it("resets preparation and errors for a new session and hides on stop", () => {
    const state = createSessionStatus();
    state.visibility(true);
    state.translation({ displayState: "warming" });
    expect(state.view().text).toBe("Preparing translation…");
    state.translation({ displayState: "stopped" });
    expect(state.view()).toBeNull();
    state.capture({ isError: true });
    state.visibility(false);
    expect(state.view()).toBeNull();
    state.visibility(true);
    expect(state.view()).toEqual({ text: "Preparing translation…", error: false });
  });
});

describe("overlay session status adapter", () => {
  afterEach(() => vi.unstubAllGlobals());

  it("uses text-safe rendering, one stable alert and the guarded Home action", async () => {
    const listeners = new Map();
    const elements = Object.fromEntries(
      [
        "overlay-session-status",
        "overlay-session-status-text",
        "overlay-session-status-action",
        "exit-translation-button",
      ].map((name) => [
        name,
        {
          hidden: false,
          style: {},
          offsetWidth: 180,
          offsetHeight: 60,
          textContent: "",
          setAttribute: vi.fn(),
          classList: { toggle: vi.fn(), contains: () => false },
          addEventListener: (kind, handler) => listeners.set(`${name}:${kind}`, handler),
          click: vi.fn(),
        },
      ]),
    );
    vi.stubGlobal("document", {
      documentElement: {},
      querySelector: () => null,
      getElementById: (name) => elements[name],
    });
    vi.stubGlobal("window", {
      innerWidth: 1280,
      innerHeight: 720,
      addEventListener: vi.fn(),
      OverlayGeometry: { resolveToolbarPlacement: vi.fn(() => ({ left: 100, top: 50 })) },
      __TAURI__: { event: { listen: async (name, callback) => listeners.set(name, callback) } },
    });
    vi.stubGlobal(
      "ResizeObserver",
      class {
        observe() {}
      },
    );
    const onChange = vi.fn();
    const onInteraction = vi.fn();
    let region = { x: 100, y: 180, width: 350, height: 100 };
    const adapter = await setupSessionStatus({ onChange, onInteraction, getRegion: () => region });
    adapter.visibility(true);
    expect(elements["overlay-session-status-text"].textContent).toBe("Preparing translation…");
    expect(elements["overlay-session-status-action"].hidden).toBe(true);
    setUiLanguage("zh-CN");
    expect(elements["overlay-session-status-text"].textContent).toBe("正在准备翻译…");
    setUiLanguage("en");
    listeners.get("capture-status")({
      payload: { isError: true, message: "<script>private</script>" },
    });
    expect(elements["overlay-session-status-text"].textContent).toBe("Translation is interrupted.");
    expect(elements["overlay-session-status-action"].hidden).toBe(false);
    const calls = onChange.mock.calls.length;
    listeners.get("capture-status")({ payload: { isError: true } });
    expect(onChange).toHaveBeenCalledTimes(calls);
    listeners.get("overlay-session-status-action:click")();
    expect(elements["exit-translation-button"].click).toHaveBeenCalledOnce();
    listeners.get("overlay-session-status:mouseenter")();
    expect(onInteraction).toHaveBeenCalledOnce();
    const beforeClear = onChange.mock.calls.length;
    listeners.get("subtitle-activity")({ payload: { state: "active" } });
    expect(elements["overlay-session-status"].hidden).toBe(true);
    expect(onChange).toHaveBeenCalledTimes(beforeClear + 1);
    adapter.translation(payload("warming"));
    expect(elements["overlay-session-status"].hidden).toBe(false);
    expect(elements["overlay-session-status"].style.left).toBe("100px");
    listeners.get("translation-interrupted")({ payload: { sessionId: 1 } });
    listeners.get("subtitle-activity")({
      payload: { sessionId: 1, captureId: 2, state: "active" },
    });
    expect(elements["overlay-session-status-text"].textContent).toBe("Translation is interrupted.");
    expect(elements["overlay-session-status-action"].hidden).toBe(false);
    region = null;
    adapter.reposition();
    expect(elements["overlay-session-status"].hidden).toBe(true);
    region = { x: 0, y: 0, width: 1280, height: 720 };
    globalThis.window.OverlayGeometry.resolveToolbarPlacement.mockReturnValue(null);
    adapter.reposition();
    expect(elements["overlay-session-status"].hidden).toBe(true);
    adapter.visibility(false);
    expect(elements["overlay-session-status"].hidden).toBe(true);
  });
});
