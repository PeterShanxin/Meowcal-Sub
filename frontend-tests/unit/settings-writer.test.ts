import { afterEach, describe, expect, it, vi } from "vitest";
import { SettingsWriter, type SettingsSaveState } from "../../src/ui/settings-writer";
import { defaultSettings } from "../../src/ui/settings-defaults";
import { acceptSavedUiLanguage, getLocale, previewUiLanguage } from "../../src/i18n/index";

afterEach(() => vi.unstubAllGlobals());

describe("SettingsWriter", () => {
  it("keeps the latest preview during queued saves and retries a failed dedicated language write", async () => {
    let fail = true;
    const writes: string[] = [];
    const invoke = vi.fn(async (command: string, args: { language?: string }) => {
      if (command === "set_ui_language") {
        if (fail) throw new Error("disk locked");
        writes.push(args.language!);
        acceptSavedUiLanguage(args.language);
      }
    });
    vi.stubGlobal("window", { TauriBridge: { invoke } });
    const states: SettingsSaveState[] = [];
    const writer = new SettingsWriter((state) => states.push(state));
    previewUiLanguage("zh-CN");
    await expect(writer.saveLanguage("zh-CN")).rejects.toThrow("disk locked");
    acceptSavedUiLanguage("en");
    expect(getLocale()).toBe("zh-CN");
    expect(states.at(-1)?.kind).toBe("error");
    fail = false;
    const first = writer.retry();
    previewUiLanguage("en");
    const second = writer.saveLanguage("en");
    previewUiLanguage("zh-CN");
    const third = writer.saveLanguage("zh-CN");
    await Promise.all([first, second, third]);
    expect(writes).toEqual(["zh-CN", "en", "zh-CN"]);
    expect(getLocale()).toBe("zh-CN");
    expect(states.at(-1)?.kind).toBe("idle");
    expect(invoke.mock.calls.every(([command]) => command === "set_ui_language")).toBe(true);
    await writer.save(defaultSettings);
    expect(writes).toHaveLength(3);
    acceptSavedUiLanguage("en");
    expect(getLocale()).toBe("en");
  });
  it("does not replay a stale subtitle pair when only the UI language changes", async () => {
    const live = { ...structuredClone(defaultSettings), uiLanguage: "en" };
    const invoke = vi.fn(async (command: string, args: Record<string, unknown>) => {
      if (command === "save_settings") Object.assign(live, args.settings);
      if (command === "set_ui_language") live.uiLanguage = String(args.language);
    });
    vi.stubGlobal("window", { TauriBridge: { invoke } });
    const writer = new SettingsWriter(() => {});
    await writer.save(defaultSettings);
    Object.assign(live, { sourceLanguage: "ja-JP", targetLanguage: "en-US" });
    invoke.mockClear();
    await writer.saveLanguage("zh-CN");
    await writer.retry();
    expect(invoke.mock.calls).toEqual([["set_ui_language", { language: "zh-CN" }]]);
    expect(live).toMatchObject({
      sourceLanguage: "ja-JP",
      targetLanguage: "en-US",
      uiLanguage: "zh-CN",
    });
  });

  it("orders mixed edits and retries each outstanding field without replaying successful writes", async () => {
    let failLanguage = true;
    const invoke = vi.fn(async (command: string) => {
      if (command === "set_ui_language" && failLanguage) throw new Error("locked");
    });
    vi.stubGlobal("window", { TauriBridge: { invoke } });
    const writer = new SettingsWriter(() => {});
    const first = writer.save({ ...defaultSettings, sourceLanguage: "ja-JP" });
    const second = writer.saveLanguage("zh-CN");
    await first;
    await expect(second).rejects.toThrow("locked");
    failLanguage = false;
    await writer.retry();
    const third = writer.saveLanguage("en");
    const fourth = writer.save({ ...defaultSettings, sourceLanguage: "ko-KR" });
    await Promise.all([third, fourth]);
    expect(invoke.mock.calls.map(([command]) => command)).toEqual([
      "save_settings",
      "set_ui_language",
      "set_ui_language",
      "set_ui_language",
      "save_settings",
    ]);
  });

  it("keeps a debounced appearance edit pending after a language-only save", async () => {
    vi.stubGlobal("window", { TauriBridge: { invoke: vi.fn().mockResolvedValue(undefined) } });
    const states: SettingsSaveState[] = [];
    const writer = new SettingsWriter((state) => states.push(state));
    writer.markPending();
    await writer.saveLanguage("zh-CN");
    expect(states.at(-1)?.kind).toBe("saving");
    await writer.save(defaultSettings);
    expect(states.at(-1)?.kind).toBe("idle");
  });
  it("continues after a rejected write and only reports the latest result", async () => {
    let reject!: (reason: unknown) => void;
    const first = new Promise<void>((_, fail) => {
      reject = fail;
    });
    const invoke = vi.fn().mockReturnValueOnce(first).mockResolvedValue(undefined);
    vi.stubGlobal("window", { TauriBridge: { invoke } });
    const states: SettingsSaveState[] = [];
    const writer = new SettingsWriter((state) => states.push(state));
    const failed = writer.save(defaultSettings).catch(() => {});
    const latest = writer.save({ ...defaultSettings, sourceLanguage: "ja-JP" });
    reject(new Error("disk full"));
    await Promise.all([failed, latest]);
    expect(states).toEqual([{ kind: "saving" }, { kind: "saving" }, { kind: "idle" }]);
    expect(invoke.mock.calls.at(-1)?.[1].settings.sourceLanguage).toBe("ja-JP");
  });

  it("owns a snapshot of each write and reports non-Error rejections", async () => {
    const invoke = vi.fn().mockRejectedValue("read-only volume");
    vi.stubGlobal("window", { TauriBridge: { invoke } });
    const states: SettingsSaveState[] = [];
    const writer = new SettingsWriter((state) => states.push(state));
    const settings = structuredClone(defaultSettings);
    const write = writer.save(settings);
    settings.overlay.fontSize = 48;
    await expect(write).rejects.toBe("read-only volume");
    expect(invoke.mock.calls[0][1].settings.overlay.fontSize).toBe(28);
    expect(states.at(-1)).toEqual({ kind: "error", message: "read-only volume" });
  });
});
