import type { AppSettings, OcrConfig } from "./contracts";
import { ensureDistinctLanguagePair } from "./languages";

type RecognitionPreset = Omit<OcrConfig, "engine">;

const balanced: RecognitionPreset = {
  confidenceThreshold: 0.5,
  preprocessingEnabled: true,
  grayscale: true,
  contrastEnhancement: true,
  binarize: true,
  enableMultiPass: false,
  multiPassCount: 2,
  validationStrictness: "moderate",
};

export const defaultOcr: OcrConfig = { ...balanced, engine: "ppOcrV6" };

/** Quality presets leave the engine choice alone. */
export const recognitionPresets = {
  fast: { ...balanced, preprocessingEnabled: false, validationStrictness: "permissive" },
  balanced,
  accurate: {
    ...balanced,
    enableMultiPass: true,
    multiPassCount: 2,
    validationStrictness: "strict",
  },
} satisfies Record<string, RecognitionPreset>;

export const defaultSettings: AppSettings = {
  sourceLanguage: "zh-CN",
  targetLanguage: "en-US",
  // Must track `default_config()` in src-tauri/src/config.rs so frontend defaults don't override backend.
  captureIntervalMs: 250,
  overlay: {
    fontSize: 28,
    fontFamily: "Segoe UI",
    textColor: "#FFFFFF",
    backgroundColor: "rgba(0, 0, 0, 0.72)",
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
    ocr: defaultOcr,
  },
  minimizeToTray: true,
  autoCheckUpdates: true,
  lastUpdateCheckTimeMs: null,
};

export function mergeSettings(value: Partial<AppSettings> | null): AppSettings {
  if (!value) return structuredClone(defaultSettings);
  const base = defaultSettings.translation;
  const lastCheck = value.lastUpdateCheckTimeMs;
  return ensureDistinctLanguagePair({
    ...structuredClone(defaultSettings),
    ...value,
    autoCheckUpdates: value.autoCheckUpdates !== false,
    lastUpdateCheckTimeMs:
      typeof lastCheck === "number" && Number.isFinite(lastCheck) ? lastCheck : null,
    overlay: { ...defaultSettings.overlay, ...(value.overlay ?? {}) },
    translation: {
      ...base,
      ...(value.translation ?? {}),
      localEngine: { ...base.localEngine, ...(value.translation?.localEngine ?? {}) },
      ocr: { ...defaultOcr, ...(value.translation?.ocr ?? {}) },
    },
  });
}
