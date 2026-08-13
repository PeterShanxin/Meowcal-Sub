import { describe, expect, it, vi } from "vitest";
import type { UiSnapshot } from "../../src/ui/contracts";
import { deriveHomePresentation } from "../../src/ui/home-state";

vi.stubGlobal("window", {
  OcrLanguageTags: {
    isOcrLanguageAvailable(installed: ReadonlySet<string>, selected: string) {
      return installed.has(selected);
    },
  },
});

function snapshot(patch: Partial<UiSnapshot> = {}): UiSnapshot {
  return {
    screen: "home",
    busy: "idle",
    settings: {
      sourceLanguage: "zh-CN",
      targetLanguage: "en-US",
      captureIntervalMs: 500,
      overlay: {
        fontSize: 32,
        fontFamily: "Segoe UI",
        textColor: "#fff",
        backgroundColor: "rgba(0,0,0,0.72)",
        offsetY: 10,
        maxWidth: 0,
        showDiagnostics: false,
      },
      translation: {
        enableLocalEngine: true,
        allowMockFallback: false,
        enableContextAware: false,
        contextLevel: "off",
        contextRecentCount: 3,
        contextBudgetPercent: 15,
        contextSummaryCooldownMs: 5000,
        promptMaxSourceChars: 300,
        promptMaxContextChars: 600,
        contextBufferSize: 12,
        contextResetGapMs: 6000,
        localEngine: { model: null, timeoutMs: 30000 },
        ocr: {
          confidenceThreshold: 0.5,
          preprocessingEnabled: true,
          grayscale: true,
          contrastEnhancement: true,
          binarize: true,
          enableMultiPass: false,
          multiPassCount: 2,
          validationStrictness: "moderate",
        },
      },
      minimizeToTray: true,
    },
    region: { x: 0, y: 0, width: 1000, height: 120 },
    engine: { phase: "ready", serviceRunning: true },
    ocrLanguages: new Set(["zh-CN"]),
    running: false,
    error: null,
    notice: null,
    developerMode: false,
    update: { kind: "idle" },
    appVersion: "0.6.6",
    ...patch,
  };
}

describe("deriveHomePresentation", () => {
  it.each([
    [snapshot({ busy: "loading" }), "none", "Checking"],
    [snapshot({ running: true }), "stop", "Running"],
    [snapshot({ running: true, busy: "stopping" }), "stop", "Running"],
    [snapshot({ busy: "warming" }), "none", "Starting"],
    [snapshot({ busy: "starting" }), "none", "Starting"],
    [snapshot({ engine: { phase: "notInstalled" } }), "setup", "Not ready"],
    [snapshot({ engine: { phase: "notinstalled" } }), "setup", "Not ready"],
    [
      snapshot({ engine: { phase: "error", supportCode: "ENGINE_BROKEN" } }),
      "repair",
      "Needs attention",
    ],
    [snapshot({ engine: { phase: "noModels" } }), "repair", "Needs attention"],
    [snapshot({ engine: { phase: "preparing" } }), "none", "Preparing"],
    [snapshot({ engine: { phase: "unknown" } }), "repair", "Needs attention"],
    [snapshot({ engine: { phase: "unexpected" } }), "repair", "Needs attention"],
    [snapshot({ error: "save failed" }), "start", "Ready"],
    [snapshot({ ocrLanguages: new Set() }), "installOcr", "Almost ready"],
    [snapshot({ region: null }), "selectRegion", "Almost ready"],
    [snapshot({ engine: { phase: "notRunning" } }), "start", "Ready"],
    [snapshot({ engine: { phase: "notrunning" } }), "start", "Ready"],
  ])("maps runtime state to one action", (input, action, statusLabel) => {
    const result = deriveHomePresentation(input);
    expect(result.action).toBe(action);
    expect(result.statusLabel).toBe(statusLabel);
    expect(result.actionLabel).toBeTruthy();
    expect(result.supportLine).toBeTruthy();
  });

  it("keeps preparing disabled and never offers start", () => {
    const result = deriveHomePresentation(snapshot({ engine: { phase: "preparing" } }));

    expect(result).toMatchObject({
      state: "checking",
      action: "none",
      actionDisabled: true,
      statusLabel: "Preparing",
    });
  });

  it.each(["unknown", "unexpected", "", undefined])(
    "routes phase %j to attention instead of Ready",
    (phase) => {
      const result = deriveHomePresentation(snapshot({ engine: { phase } }));

      expect(result).toMatchObject({
        state: "attention",
        action: "repair",
        statusLabel: "Needs attention",
      });
    },
  );

  it("routes a missing engine status to attention instead of Ready", () => {
    const result = deriveHomePresentation(snapshot({ engine: null }));

    expect(result).toMatchObject({
      state: "attention",
      action: "repair",
      statusLabel: "Needs attention",
    });
  });

  it.each(["ready", "notRunning", "notrunning"])(
    "allows valid readiness phase %s to progress to the start action",
    (phase) => {
      const result = deriveHomePresentation(snapshot({ engine: { phase } }));

      expect(result).toMatchObject({ state: "ready", action: "start", statusLabel: "Ready" });
    },
  );
});
