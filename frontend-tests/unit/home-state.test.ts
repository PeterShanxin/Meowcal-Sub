import { describe, expect, it, vi } from "vitest";
import type { UiSnapshot } from "../../src/ui/contracts";
import { deriveHomePresentation, isSessionLocked } from "../../src/ui/home-state";

vi.stubGlobal("window", {
  OcrLanguageTags: {
    isOcrLanguageAvailable(installed: ReadonlySet<string>, selected: string) {
      return installed.has(selected);
    },
  },
});

it("shows busy work without offering repair or claiming another model load", () => {
  const home = deriveHomePresentation(snapshot({ engine: { phase: "busy" } }));
  expect(home).toMatchObject({
    state: "checking",
    title: "Waiting for the engine",
    action: "none",
    actionDisabled: true,
  });
});

function snapshot(patch: Partial<UiSnapshot> = {}): UiSnapshot {
  return {
    settingsSave: { kind: "idle" },
    screen: "home",
    busy: "idle",
    multipleDisplays: false,
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
        lightBackground: false,
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
        translateAllOcrText: false,
        localEngine: { model: null, timeoutMs: 30000, cpuOnly: false },
        ocr: {
          confidenceThreshold: 0.5,
          preprocessingEnabled: true,
          grayscale: true,
          contrastEnhancement: true,
          binarize: true,
          enableMultiPass: false,
          multiPassCount: 2,
          validationStrictness: "moderate",
          engine: "windows",
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
    captureWarning: null,
    developerMode: false,
    update: { kind: "idle" },
    appVersion: "0.6.6",
    ...patch,
  };
}

describe("recognition readiness", () => {
  const withEngine = (engine: "ppOcrV6" | "windows", sourceLanguage = "zh-CN") => {
    const base = snapshot({ ocrLanguages: new Set() });
    return {
      ...base,
      settings: {
        ...base.settings,
        sourceLanguage,
        translation: {
          ...base.settings.translation,
          ocr: { ...base.settings.translation.ocr, engine },
        },
      },
    };
  };

  it("needs no Windows language pack when PP-OCRv6 reads the subtitles", () => {
    expect(deriveHomePresentation(withEngine("ppOcrV6")).action).toBe("start");
    expect(deriveHomePresentation(withEngine("windows")).action).toBe("installOcr");
  });

  it("asks for the Windows language pack for Korean even with PP-OCRv6 selected", () => {
    expect(deriveHomePresentation(withEngine("ppOcrV6", "ko-KR")).action).toBe("installOcr");
  });
});

describe("deriveHomePresentation", () => {
  it.each([
    [snapshot({ busy: "loading" }), "none", "Checking"],
    [snapshot({ running: true }), "stop", "Running"],
    [snapshot({ running: true, busy: "stopping" }), "stop", "Running"],
    [snapshot({ busy: "selecting" }), "none", "Select area"],
    [snapshot({ busy: "warming" }), "none", "Starting"],
    [snapshot({ busy: "starting" }), "none", "Starting"],
    [snapshot({ engine: { phase: "notInstalled" } }), "setup", "Setup needed"],
    [snapshot({ engine: { phase: "notinstalled" } }), "setup", "Setup needed"],
    [
      snapshot({ engine: { phase: "error", supportCode: "ENGINE_BROKEN" } }),
      "repair",
      "Needs repair",
    ],
    [snapshot({ engine: { phase: "noModels" } }), "repair", "Needs repair"],
    [snapshot({ engine: { phase: "preparing" } }), "none", "Preparing"],
    [snapshot({ engine: { phase: "unknown" } }), "repair", "Needs repair"],
    [snapshot({ engine: { phase: "unexpected" } }), "repair", "Needs repair"],
    [snapshot({ engine: { phase: "backendUnavailable" } }), "none", "Backend offline"],
    [snapshot({ error: "save failed" }), "start", "Ready"],
    [snapshot({ ocrLanguages: new Set() }), "installOcr", "Almost ready"],
    [snapshot({ region: null }), "start", "Almost ready"],
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
        statusLabel: "Needs repair",
      });
    },
  );

  it("keeps a repair support code out of the sentence so it can be read out exactly", () => {
    const result = deriveHomePresentation(
      snapshot({ engine: { phase: "error", supportCode: "ENGINE_HEALTH_FAILED" } }),
    );

    expect(result).toMatchObject({
      supportLine: "Support code",
      supportCode: "ENGINE_HEALTH_FAILED",
      supportTone: "danger",
    });
  });

  it("routes a missing engine status to attention instead of Ready", () => {
    const result = deriveHomePresentation(snapshot({ engine: null }));

    expect(result).toMatchObject({
      state: "attention",
      action: "repair",
      statusLabel: "Needs repair",
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

describe("Home capture warnings", () => {
  it("keeps a healthy session running with stable support text", () => {
    const result = deriveHomePresentation(snapshot({ running: true }));
    expect(result).toMatchObject({
      state: "running",
      statusLabel: "Running",
      action: "stop",
      actionDisabled: false,
      supportLine: "Overlay active · Local processing",
      supportTone: "success",
    });
  });

  it("shows a non-fatal capture report on a running session", () => {
    const result = deriveHomePresentation(
      snapshot({
        running: true,
        captureWarning: "Using GDI fallback - video content may not capture correctly",
      }),
    );

    expect(result).toMatchObject({
      state: "running",
      supportLine: "Compatibility capture · Protected video may not be read",
      supportTone: "warning",
    });
  });
});

describe("Home session controls", () => {
  it.each(["selecting", "warming", "starting", "stopping"] as const)(
    "locks the language pair and area while translation is %s",
    (busy) => {
      expect(isSessionLocked(snapshot({ busy, running: busy === "stopping" }))).toBe(true);
    },
  );

  it("locks them while translation runs and unlocks them when it is ready", () => {
    expect(isSessionLocked(snapshot({ running: true }))).toBe(true);
    expect(isSessionLocked(snapshot())).toBe(false);
  });
});
