import { afterEach, describe, expect, it, vi } from "vitest";
import { SettingsWriter, type SettingsSaveState } from "../../src/ui/settings-writer";
import { defaultSettings } from "../../src/ui/settings-defaults";

afterEach(() => vi.unstubAllGlobals());

describe("SettingsWriter", () => {
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
