import { afterEach, expect, it, vi } from "vitest";
import { AppController } from "../../src/ui/app-controller";
import {
  confirmUiLanguage,
  getLocale,
  getUiLanguage,
  previewUiLanguage,
  setUiLanguage,
} from "../../src/i18n/index";
import { initializeLocale } from "../../src/i18n/runtime";

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((done) => (resolve = done));
  return { promise, resolve };
}

function bridge(invoke: (command: string, args?: unknown) => Promise<unknown>, native = false) {
  const events = Object.assign(new EventTarget(), {
    TauriBridge: {
      invoke,
      isBrowserMode: () => !native,
      event: { listen: vi.fn(async () => () => {}), emit: vi.fn(async () => {}) },
    },
    setTimeout,
    clearTimeout,
    setInterval,
    clearInterval,
  });
  vi.stubGlobal("window", events);
  vi.stubGlobal("navigator", { languages: ["en-US"] });
  vi.stubGlobal("localStorage", { getItem: () => null, setItem: vi.fn() });
  return events;
}

afterEach(() => vi.unstubAllGlobals());

it("localizes the browser badge created after initialization and on live switches", async () => {
  const events = bridge(async () => ({ uiLanguage: "zh-CN" }));
  const indicator = { textContent: "Browser preview", title: "" };
  let attached = false;
  vi.stubGlobal("document", {
    documentElement: { lang: "en" },
    title: "",
    getElementById: () => (attached ? indicator : null),
  });
  const dispose = await initializeLocale();
  attached = true;
  events.dispatchEvent(new Event("DOMContentLoaded"));
  expect(indicator.textContent).toBe("浏览器预览");
  expect(indicator.title).toBe("屏幕捕获、翻译和原生窗口需要使用 Windows 应用。");
  setUiLanguage("en");
  expect(indicator.textContent).toBe("Browser preview");
  expect(indicator.title).toBe("Capture, translation, and native windows require the Windows app.");
  dispose();
  setUiLanguage("zh-CN");
  expect(indicator.textContent).toBe("Browser preview");
});

it("blocks language edits while startup is loading and never reapplies its stale preference", async () => {
  const engine = deferred<unknown>();
  const invoke = vi.fn(async (command: string) => {
    if (command === "get_settings") return { uiLanguage: "en", sourceLanguage: "ja-JP" };
    if (command === "get_engine_status") return engine.promise;
    if (command === "get_ocr_languages") return [];
    return undefined;
  });
  bridge(invoke);
  const controller = new AppController(() => {});
  try {
    const initializing = controller.initialize();
    await vi.waitFor(() => expect(invoke).toHaveBeenCalledWith("get_engine_status"));
    controller.setScreen("settings");
    await controller.setUiLanguage("zh-CN");
    expect(getUiLanguage()).toBe("en");
    expect(invoke.mock.calls.some(([command]) => command === "save_settings")).toBe(false);

    // A locale runtime or another webview can still supply a newer preference.
    confirmUiLanguage("zh-CN");
    engine.resolve({ phase: "ready" });
    await initializing;
    expect(getUiLanguage()).toBe("zh-CN");
    expect(controller.current().busy).toBe("idle");

    await controller.setUiLanguage("en");
    expect(getUiLanguage()).toBe("en");
    expect(invoke).toHaveBeenCalledWith("set_ui_language", { language: "en" });
    expect(controller.current().settings.sourceLanguage).toBe("ja-JP");
  } finally {
    controller.dispose();
  }
});

it("a completed local edit wins over the locale runtime's delayed initial settings read", async () => {
  const settings = deferred<unknown>();
  const invoke = vi.fn(() => settings.promise);
  bridge(invoke);
  const initializing = initializeLocale();
  await vi.waitFor(() => expect(invoke).toHaveBeenCalledWith("get_settings"));
  previewUiLanguage("zh-CN");
  confirmUiLanguage("zh-CN");
  settings.resolve({ uiLanguage: "en" });
  const dispose = await initializing;
  expect(getUiLanguage()).toBe("zh-CN");
  expect(getLocale()).toBe("zh-CN");
  dispose();
});

it("the latest system-language notification wins when native reads resolve out of order", async () => {
  const first = deferred<string>();
  const second = deferred<string>();
  const readSystem = vi
    .fn()
    .mockResolvedValueOnce("en")
    .mockReturnValueOnce(first.promise)
    .mockReturnValueOnce(second.promise);
  const events = bridge(
    async (command) =>
      command === "get_system_ui_language" ? readSystem() : { uiLanguage: "system" },
    true,
  );
  const dispose = await initializeLocale();
  events.dispatchEvent(new Event("languagechange"));
  events.dispatchEvent(new Event("languagechange"));
  second.resolve("zh-CN");
  await vi.waitFor(() => expect(getLocale()).toBe("zh-CN"));
  first.resolve("en");
  await new Promise((resolve) => setTimeout(resolve, 0));
  expect(getLocale()).toBe("zh-CN");
  dispose();
});

it("disposing the locale runtime prevents an in-flight system-language read from applying", async () => {
  const pending = deferred<string>();
  const readSystem = vi.fn().mockResolvedValueOnce("en").mockReturnValueOnce(pending.promise);
  const events = bridge(
    async (command) =>
      command === "get_system_ui_language" ? readSystem() : { uiLanguage: "system" },
    true,
  );
  const dispose = await initializeLocale();
  events.dispatchEvent(new Event("languagechange"));
  dispose();
  setUiLanguage("system", ["en"]);
  pending.resolve("zh-CN");
  await new Promise((resolve) => setTimeout(resolve, 0));
  expect(getLocale()).toBe("en");
});
