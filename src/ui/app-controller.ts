import type {
  AppScreen,
  AppSettings,
  CaptureRegion,
  EngineStatus,
  OcrConfig,
  UiSnapshot,
} from "./contracts";
import { pickSampleTranslation } from "./sample-translations";
import { applyLanguageSelection, ensureDistinctLanguagePair } from "./languages";
import { UpdateController } from "./update-controller";

type Subscriber = (snapshot: UiSnapshot) => void;

const ONBOARDING_COMPLETE_KEY = "meowcal.onboardingComplete";

const defaultOcr: OcrConfig = {
  confidenceThreshold: 0.5,
  preprocessingEnabled: true,
  grayscale: true,
  contrastEnhancement: true,
  binarize: true,
  enableMultiPass: false,
  multiPassCount: 2,
  validationStrictness: "moderate",
};

const defaultSettings: AppSettings = {
  sourceLanguage: "zh-CN",
  targetLanguage: "en-US",
  // Must track `default_config()` in src-tauri/src/config.rs. The frontend
  // saves this whole object back over the stored settings, so a stale value
  // here silently overrides the backend default - which is how a 500ms capture
  // interval kept coming back after the backend moved to 250ms.
  captureIntervalMs: 250,
  overlay: {
    fontSize: 32,
    fontFamily: "Segoe UI",
    textColor: "#FFFFFF",
    backgroundColor: "rgba(0, 0, 0, 0.72)",
    offsetY: 10,
    maxWidth: 0,
    showDiagnostics: false,
  },
  translation: {
    enableFoundryLocal: true,
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
    foundryLocal: { model: null, timeoutMs: 30000 },
    ocr: defaultOcr,
  },
  autoStart: false,
  minimizeToTray: true,
  startWithWindows: false,
};

function mergeSettings(value: Partial<AppSettings> | null): AppSettings {
  if (!value) return structuredClone(defaultSettings);
  return ensureDistinctLanguagePair({
    ...structuredClone(defaultSettings),
    ...value,
    overlay: { ...defaultSettings.overlay, ...(value.overlay ?? {}) },
    translation: {
      ...defaultSettings.translation,
      ...(value.translation ?? {}),
      foundryLocal: {
        ...defaultSettings.translation.foundryLocal,
        ...(value.translation?.foundryLocal ?? {}),
      },
      ocr: { ...defaultOcr, ...(value.translation?.ocr ?? {}) },
    },
  });
}

function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

export class AppController {
  private subscriber: Subscriber;
  private unlisten: Array<() => void> = [];
  private pollingId: number | null = null;
  private overlaySaveId: number | null = null;
  private snapshot: UiSnapshot = {
    screen: "home",
    busy: "loading",
    settings: structuredClone(defaultSettings),
    region: null,
    engine: null,
    ocrLanguages: new Set(),
    running: false,
    error: null,
    notice: null,
    developerMode: localStorage.getItem("meowcal.developerMode") === "true",
    update: { kind: "idle" },
    appVersion: null,
  };
  private updates = new UpdateController((patch) => this.publish(patch));

  constructor(subscriber: Subscriber) {
    this.subscriber = subscriber;
  }

  current(): UiSnapshot {
    return this.snapshot;
  }

  private publish(patch: Partial<UiSnapshot>): void {
    this.snapshot = { ...this.snapshot, ...patch };
    this.subscriber(this.snapshot);
  }

  async initialize(): Promise<void> {
    this.publish({ busy: "loading", error: null });
    const browserMode = window.TauriBridge.isBrowserMode();
    const [settings, languages, engine, region, running] = await Promise.all([
      this.safeInvoke<Partial<AppSettings> | null>("get_settings", null),
      this.safeInvoke<string[]>("get_ocr_languages", []),
      this.safeInvoke<EngineStatus>("get_foundry_local_status", { phase: "unknown" }),
      this.safeInvoke<CaptureRegion | null>("get_capture_region", null),
      browserMode
        ? Promise.resolve(false)
        : this.safeInvoke<boolean>("is_translation_running", false),
    ]);
    const merged = mergeSettings(settings);
    this.publish({
      settings: merged,
      ocrLanguages: new Set(languages),
      engine,
      region: region ?? merged.lastCaptureRegion ?? null,
      running,
      busy: "idle",
      ...(await this.updates.initialState()),
    });
    await this.setupEvents();
    if (!browserMode && localStorage.getItem(ONBOARDING_COMPLETE_KEY) !== "true") {
      await this.openSetup();
    }
  }

  private async safeInvoke<T>(command: string, fallback: T): Promise<T> {
    try {
      return await window.TauriBridge.invoke<T>(command);
    } catch (error) {
      console.warn(`[Meowcal] ${command} unavailable`, error);
      return fallback;
    }
  }

  private async setupEvents(): Promise<void> {
    const regionUnlisten = await window.TauriBridge.event.listen("region-selected", (event) => {
      this.stopRegionPolling();
      this.publish({ region: event.payload as CaptureRegion, notice: "Subtitle area selected" });
    });
    const captureUnlisten = await window.TauriBridge.event.listen("capture-status", (event) => {
      const payload = event.payload as { isError?: boolean; message?: string };
      if (payload.isError) this.publish({ error: payload.message ?? "Screen capture failed" });
    });
    const wizardUnlisten = await window.TauriBridge.event.listen(
      "foundry-wizard-closed",
      (event) => {
        const payload = event.payload as { modelDownloaded?: boolean } | null;
        if (payload?.modelDownloaded === true) {
          localStorage.setItem(ONBOARDING_COMPLETE_KEY, "true");
        }
      },
    );
    this.unlisten.push(regionUnlisten, captureUnlisten, wizardUnlisten);
  }

  dispose(): void {
    this.stopRegionPolling();
    if (this.overlaySaveId !== null) window.clearTimeout(this.overlaySaveId);
    this.unlisten.splice(0).forEach((callback) => callback());
  }

  setScreen(screen: AppScreen): void {
    this.publish({ screen, notice: null });
  }

  async setLanguage(kind: "source" | "target", value: string): Promise<void> {
    const settings = applyLanguageSelection(structuredClone(this.snapshot.settings), kind, value);
    this.publish({ settings, notice: null });
    await this.persistSettingsInBackground();
  }

  async selectRegion(): Promise<void> {
    try {
      await window.TauriBridge.invoke("open_area_selector");
      this.startRegionPolling();
    } catch (error) {
      this.publish({ error: errorMessage(error) });
    }
  }

  private startRegionPolling(): void {
    this.stopRegionPolling();
    let attempts = 0;
    this.pollingId = window.setInterval(async () => {
      attempts += 1;
      const region = await this.safeInvoke<CaptureRegion | null>("get_capture_region", null);
      if (region) {
        this.stopRegionPolling();
        this.publish({ region, notice: "Subtitle area selected" });
      } else if (attempts >= 40) {
        this.stopRegionPolling();
      }
    }, 250);
  }

  private stopRegionPolling(): void {
    if (this.pollingId !== null) window.clearInterval(this.pollingId);
    this.pollingId = null;
  }

  async installOcr(): Promise<void> {
    this.publish({ busy: "saving", error: null, notice: "Opening Windows language setup…" });
    try {
      await window.TauriBridge.invoke("install_ocr_language", {
        languageTag: this.snapshot.settings.sourceLanguage,
      });
      const languages = await window.TauriBridge.invoke<string[]>("get_ocr_languages");
      this.publish({
        ocrLanguages: new Set(languages),
        busy: "idle",
        notice: "OCR check complete",
      });
    } catch (error) {
      this.publish({ busy: "idle", error: errorMessage(error) });
    }
  }

  async openSetup(): Promise<void> {
    try {
      await window.TauriBridge.invoke("open_foundry_wizard");
    } catch (error) {
      this.publish({ error: errorMessage(error) });
    }
  }

  async refresh(): Promise<void> {
    const [engine, region] = await Promise.all([
      this.safeInvoke<EngineStatus>("refresh_foundry_local_status", this.snapshot.engine ?? {}),
      this.safeInvoke<CaptureRegion | null>("get_capture_region", this.snapshot.region),
    ]);
    this.publish({ engine, region, error: null });
  }

  async start(): Promise<void> {
    this.publish({ busy: "warming", error: null, notice: null });
    try {
      await this.saveSettings(true);
      let engine = await window.TauriBridge.invoke<EngineStatus>("refresh_foundry_local_status");
      if (["notRunning", "notrunning", "preparing"].includes(engine.phase ?? "")) {
        engine = await window.TauriBridge.invoke<EngineStatus>("make_foundry_ready");
      }
      if (engine.phase !== "ready")
        throw new Error("The local translation engine is not ready yet.");
      this.publish({ engine, busy: "starting" });
      await window.TauriBridge.invoke("start_translation");
      this.publish({ running: true, busy: "idle", notice: "Translation started" });
    } catch (error) {
      this.publish({ running: false, busy: "idle", error: errorMessage(error) });
    }
  }

  async stop(): Promise<void> {
    this.publish({ busy: "stopping", error: null });
    try {
      await window.TauriBridge.invoke("stop_translation");
      this.publish({ running: false, busy: "idle", notice: "Translation stopped" });
    } catch (error) {
      this.publish({ busy: "idle", error: errorMessage(error) });
    }
  }

  async saveSettings(silent = false): Promise<void> {
    try {
      await window.TauriBridge.invoke("save_settings", { settings: this.snapshot.settings });
      if (!silent) this.publish({ notice: "Settings saved", error: null });
    } catch (error) {
      if (!silent) this.publish({ error: errorMessage(error) });
      else throw error;
    }
  }

  private async persistSettingsInBackground(): Promise<void> {
    try {
      await this.saveSettings(true);
    } catch (error) {
      this.publish({ error: errorMessage(error) });
    }
  }

  async setRecognitionPreset(value: "fast" | "balanced" | "accurate"): Promise<void> {
    const settings = structuredClone(this.snapshot.settings);
    const presets: Record<typeof value, OcrConfig> = {
      fast: { ...defaultOcr, preprocessingEnabled: false, validationStrictness: "permissive" },
      balanced: { ...defaultOcr },
      accurate: {
        ...defaultOcr,
        enableMultiPass: true,
        multiPassCount: 2,
        validationStrictness: "strict",
      },
    };
    settings.translation.ocr = presets[value];
    this.publish({ settings });
    await this.persistSettingsInBackground();
  }

  async setContinuity(enabled: boolean): Promise<void> {
    const settings = structuredClone(this.snapshot.settings);
    settings.translation.enableContextAware = enabled;
    settings.translation.contextLevel = enabled ? "memoryAndRecent" : "off";
    this.publish({ settings });
    await this.persistSettingsInBackground();
  }

  async updateOverlay(patch: Partial<AppSettings["overlay"]>): Promise<void> {
    const settings = structuredClone(this.snapshot.settings);
    settings.overlay = { ...settings.overlay, ...patch };
    this.publish({ settings });
    try {
      await window.TauriBridge.event.emit("overlay-settings-updated", settings.overlay);
    } catch (error) {
      this.publish({ error: errorMessage(error) });
    }
    if (this.overlaySaveId !== null) window.clearTimeout(this.overlaySaveId);
    this.overlaySaveId = window.setTimeout(() => {
      this.overlaySaveId = null;
      void this.persistSettingsInBackground();
    }, 250);
  }

  async updatePreference(
    kind: "minimizeToTray" | "startWithWindows",
    enabled: boolean,
  ): Promise<void> {
    const settings = structuredClone(this.snapshot.settings);
    settings[kind] = enabled;
    this.publish({ settings });
    await this.persistSettingsInBackground();
  }

  async testTranslation(): Promise<void> {
    this.publish({ busy: "saving", notice: "Running a private sample translation…", error: null });
    try {
      const result = await window.TauriBridge.invoke<{
        translatedText?: string;
        latencyMs?: number;
      }>("wizard_test_translation", {
        sourceText: pickSampleTranslation(this.snapshot.settings.sourceLanguage),
        sourceLanguage: this.snapshot.settings.sourceLanguage,
        targetLanguage: this.snapshot.settings.targetLanguage,
      });
      if (!result.translatedText) throw new Error("The sample translation did not return text.");
      const latency = result.latencyMs ? ` · ${result.latencyMs} ms` : "";
      this.publish({ busy: "idle", notice: `Sample passed${latency}` });
    } catch (error) {
      this.publish({ busy: "idle", error: errorMessage(error) });
    }
  }

  async checkForUpdates(): Promise<void> {
    await this.updates.check();
  }

  async installUpdate(): Promise<void> {
    await this.updates.install();
  }

  setDeveloperMode(enabled: boolean): void {
    localStorage.setItem("meowcal.developerMode", String(enabled));
    this.publish({ developerMode: enabled });
  }
}
