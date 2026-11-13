import { afterEach, expect, it, vi } from "vitest";
import {
  getLocale,
  getUiLanguage,
  normalizePreference,
  resolveLocale,
  setUiLanguage,
  subscribeLocale,
  t,
  messages,
} from "../../src/i18n/index";
import { initializeLocale } from "../../src/i18n/runtime";
import { deriveUpdatePresentation } from "../../src/ui/update-state";
import { describeError } from "../../src/ui/error-copy";
import { mergeSettings } from "../../src/ui/settings-defaults";

afterEach(() => vi.unstubAllGlobals());

it("native system language comes from Windows rather than WebView regional preferences", async () => {
  vi.stubGlobal("navigator", { languages: ["en-US"] });
  vi.stubGlobal(
    "window",
    Object.assign(new EventTarget(), {
      TauriBridge: {
        isBrowserMode: () => false,
        invoke: async (command: string) =>
          command === "get_system_ui_language" ? "zh-CN" : { uiLanguage: "system" },
        event: { listen: async () => () => {} },
      },
    }),
  );
  const dispose = await initializeLocale();
  expect(getLocale()).toBe("zh-CN");
  dispose();
});

it.each([
  ["zh-CN", "zh-CN"],
  ["zh-TW", "zh-CN"],
  ["zh-Hant-HK", "zh-CN"],
  ["en-US", "en"],
  ["ja-JP", "en"],
])("resolves system %s to %s", (system, expected) => {
  expect(resolveLocale("system", [system])).toBe(expected);
  expect(resolveLocale("en", [system])).toBe("en");
  expect(resolveLocale("zh-CN", [system])).toBe("zh-CN");
});

it("uses English for unsupported or absent system languages and migrates old preferences", () => {
  expect(resolveLocale("system", [])).toBe("en");
  expect(resolveLocale("system", ["fr", "zh-CN"])).toBe("en");
  expect(normalizePreference(undefined)).toBe("system");
  expect(normalizePreference("fr")).toBe("system");
  expect(mergeSettings({ sourceLanguage: "ja-JP", targetLanguage: "zh-CN" })).toMatchObject({
    uiLanguage: "system",
    sourceLanguage: "ja-JP",
    targetLanguage: "zh-CN",
  });
});

it("switches subscribers, document metadata and interpolated copy without mutating parameters", () => {
  const document = { documentElement: { lang: "" }, title: "" };
  vi.stubGlobal("document", document);
  const changed = vi.fn();
  const stop = subscribeLocale(changed);
  setUiLanguage("zh-CN");
  expect(getLocale()).toBe("zh-CN");
  expect(document).toEqual({ documentElement: { lang: "zh-CN" }, title: "Meowcal Sub · 妙影" });
  expect(t("Version {version} is available", { version: "<b>2.0</b>" })).toBe(
    "有新版本 <b>2.0</b>",
  );
  expect(t("Version {version} is available")).toBe("有新版本 {version}");
  expect(t("unregistered diagnostic")).toBe("unregistered diagnostic");
  setUiLanguage("zh-CN");
  expect(changed).toHaveBeenCalledTimes(1);
  stop();
  setUiLanguage("en");
  expect(changed).toHaveBeenCalledTimes(1);
  expect(document.title).toBe("Meowcal Sub");
});

it("translates update progress and errors without losing support details", () => {
  setUiLanguage("zh-CN");
  expect(
    deriveUpdatePresentation({ kind: "downloading", version: "1.2", percent: 45 }, "1.1"),
  ).toMatchObject({ headline: "正在下载 1.2…", detail: "已下载 45%" });
  expect(describeError("No capture region set")).toMatchObject({
    text: "开始前请先选择字幕区域。",
    action: { kind: "selectRegion", label: "选择区域" },
  });
  expect(describeError("ENGINE_FAILED: disk locked").text).toContain("操作失败");
  expect(describeError("ENGINE_FAILED: disk locked").text).toContain("ENGINE_FAILED: disk locked");
  expect(describeError("Failed: C:/Users/小明/config.json").text).toContain("操作失败");
});

it("every catalog entry has both languages and identical named placeholders", () => {
  expect(Object.keys(messages).length).toBeGreaterThan(200);
  const placeholders = (value: string) => [...value.matchAll(/\{(\w+)\}/g)].map((m) => m[1]).sort();
  for (const [key, value] of Object.entries(messages)) {
    expect(value.en.trim(), key).not.toBe("");
    expect(value["zh-CN"].trim(), key).not.toBe("");
    expect(placeholders(value["zh-CN"]), key).toEqual(placeholders(value.en));
  }
});

it("a newer cross-window event wins over a pending initial read and listeners dispose", async () => {
  let receive!: (event: { payload: unknown }) => void;
  let resolve!: (value: unknown) => void;
  const unlisten = vi.fn();
  const events = new EventTarget();
  vi.stubGlobal(
    "window",
    Object.assign(events, {
      TauriBridge: {
        isBrowserMode: () => true,
        invoke: () =>
          new Promise((r) => {
            resolve = r;
          }),
        event: {
          listen: async (_name: string, listener: typeof receive) => {
            receive = listener;
            return unlisten;
          },
        },
      },
    }),
  );
  vi.stubGlobal("navigator", { languages: ["en-US"] });
  const pending = initializeLocale();
  await vi.waitFor(() => expect(receive).toBeTypeOf("function"));
  receive({ payload: "zh-CN" });
  resolve({ uiLanguage: "en" });
  const dispose = await pending;
  expect(getLocale()).toBe("zh-CN");
  receive({ payload: "system" });
  vi.stubGlobal("navigator", { languages: ["zh-TW"] });
  events.dispatchEvent(new Event("languagechange"));
  await vi.waitFor(() => expect(getLocale()).toBe("zh-CN"));
  expect(getUiLanguage()).toBe("system");
  expect(getLocale()).toBe("zh-CN");
  dispose();
  expect(unlisten).toHaveBeenCalledOnce();
});

it("an offline browser retains system detection", async () => {
  setUiLanguage("system", ["zh-CN"]);
  vi.stubGlobal("navigator", { languages: ["zh-CN"] });
  vi.stubGlobal(
    "window",
    Object.assign(new EventTarget(), {
      TauriBridge: {
        isBrowserMode: () => true,
        invoke: async () => {
          throw new Error("offline");
        },
        event: { listen: async () => () => {} },
      },
    }),
  );
  const dispose = await initializeLocale();
  expect(getLocale()).toBe("zh-CN");
  dispose();
});
