import type { AppScreen, AppSettings, CaptureRegion, EngineStatus, UiSnapshot } from "./contracts";
import { pickSampleTranslation } from "./sample-translations";
import { applyLanguageSelection } from "./languages";
import { defaultOcr, defaultSettings, mergeSettings } from "./settings-defaults";
import { UpdateController } from "./update-controller";

type Subscriber = (snapshot: UiSnapshot) => void;

const ONBOARDING_COMPLETE_KEY = "meowcal.onboardingComplete";

function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

type WizardTestResult = { translatedText?: string; latencyMs?: number };

export class AppController {
  private unlisten: Array<() => void> = [];
  private pollingId: number | null = null;
  private overlaySaveId: number | null = null;
  private autoCheckTimer: number | null = null;
  private settingsLoaded = false;
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

  constructor(
    private readonly subscriber: Subscriber,
    private readonly clock: () => number = () => Date.now(),
  ) {}

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
      this.safeInvoke<EngineStatus>("get_engine_status", { phase: "unknown" }),
      this.safeInvoke<CaptureRegion | null>("get_capture_region", null),
      browserMode ? false : this.safeInvoke<boolean>("is_translation_running", false),
    ]);
    this.settingsLoaded = settings !== null;
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
    this.scheduleAutomaticUpdateCheck();
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
      "engine-wizard-closed",
      (event) => {
        if ((event.payload as { modelDownloaded?: boolean } | null)?.modelDownloaded) {
          localStorage.setItem(ONBOARDING_COMPLETE_KEY, "true");
        }
      },
    );
    this.unlisten.push(regionUnlisten, captureUnlisten, wizardUnlisten);
  }

  dispose(): void {
    this.stopRegionPolling();
    if (this.overlaySaveId !== null) window.clearTimeout(this.overlaySaveId);
    if (this.autoCheckTimer !== null) window.clearTimeout(this.autoCheckTimer);
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
      const region = await this.safeInvoke<CaptureRegion | null>("get_capture_region", null);
      if (region) {
        this.stopRegionPolling();
        this.publish({ region, notice: "Subtitle area selected" });
      } else if (++attempts >= 40) this.stopRegionPolling();
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
      await window.TauriBridge.invoke("open_engine_wizard");
    } catch (error) {
      this.publish({ error: errorMessage(error) });
    }
  }

  async refresh(): Promise<void> {
    const [engine, region] = await Promise.all([
      this.safeInvoke<EngineStatus>("refresh_engine_status", this.snapshot.engine ?? {}),
      this.safeInvoke<CaptureRegion | null>("get_capture_region", this.snapshot.region),
    ]);
    this.publish({ engine, region, error: null });
  }

  async start(): Promise<void> {
    this.publish({ busy: "warming", error: null, notice: null });
    try {
      await this.saveSettings(true);
      let engine = await window.TauriBridge.invoke<EngineStatus>("refresh_engine_status");
      if (["notRunning", "notrunning", "preparing"].includes(engine.phase ?? "")) {
        engine = await window.TauriBridge.invoke<EngineStatus>("make_engine_ready");
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
      this.settingsLoaded = true;
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
    const overrides = {
      fast: { preprocessingEnabled: false, validationStrictness: "permissive" as const },
      balanced: {},
      accurate: {
        enableMultiPass: true,
        multiPassCount: 2,
        validationStrictness: "strict" as const,
      },
    };
    settings.translation.ocr = { ...defaultOcr, ...overrides[value] };
    this.publish({ settings });
    await this.persistSettingsInBackground();
  }

  async setTranslateAllOcrText(enabled: boolean): Promise<void> {
    const settings = structuredClone(this.snapshot.settings);
    settings.translation.translateAllOcrText = enabled;
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
    kind: "minimizeToTray" | "autoCheckUpdates",
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
      const result = await window.TauriBridge.invoke<WizardTestResult>("wizard_test_translation", {
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

  private scheduleAutomaticUpdateCheck(): void {
    if (this.autoCheckTimer !== null) window.clearTimeout(this.autoCheckTimer);
    this.autoCheckTimer = window.setTimeout(() => {
      this.autoCheckTimer = null;
      void this.checkForUpdatesAutomatically();
    }, 2000);
  }

  async checkForUpdatesAutomatically(): Promise<void> {
    const completedAt = await this.updates.checkAutomatic(this.snapshot.settings, this.clock);
    if (completedAt === null) return;
    const settings = structuredClone(this.snapshot.settings);
    settings.lastUpdateCheckTimeMs = completedAt;
    this.publish({ settings });
    if (!this.settingsLoaded) return;
    await this.persistSettingsInBackground();
  }

  async checkForUpdates(): Promise<void> {
    await this.updates.check("manual");
  }

  async installUpdate(): Promise<void> {
    await this.updates.install();
  }

  setDeveloperMode(enabled: boolean): void {
    localStorage.setItem("meowcal.developerMode", String(enabled));
    this.publish({ developerMode: enabled });
  }
}
