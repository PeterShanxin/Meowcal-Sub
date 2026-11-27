import { errorMessage } from "./error-copy";
import { acceptSavedUiLanguage, previewUiLanguage, type UiLanguage } from "../i18n";
import type { AppScreen, AppSettings, CaptureRegion, UiSnapshot } from "./contracts";
import { applyRecognition, type RecognitionChoice } from "./recognition";
import { pickSampleTranslation } from "./sample-translations";
import { applyLanguageSelection } from "./languages";
import { backendUnavailablePhase } from "./home-state";
import { defaultSettings, mergeSettings, recognitionPresets } from "./settings-defaults";
import { UpdateController } from "./update-controller";
import { SettingsWriter } from "./settings-writer";
import { settingsAfterRefresh, settingsSaveFeedback } from "./settings-feedback";
import { EngineStatusController } from "./engine-status-controller";
import { SessionStatusController } from "./session-status-controller";
import { DisplayController } from "./display-controller";
import { InitialSetup } from "./initial-setup";

const NOTICE_DURATION_MS = 4000;
type WizardTestResult = { translatedText?: string; latencyMs?: number };

export class AppController {
  private unlisten: Array<() => void> = [];
  private selectionRequest: string | null = null;
  private refreshRevision = 0;
  private overlaySaveId: number | null = null;
  private autoCheckTimer: number | null = null;
  private noticeTimer: number | null = null;
  private initialSetup = new InitialSetup();
  private disposed = false;
  private snapshot: UiSnapshot = {
    settingsRead: "loading",
    settingsSave: { kind: "idle" },
    screen: "home",
    busy: "loading",
    settings: structuredClone(defaultSettings),
    region: null,
    multipleDisplays: false,
    engine: null,
    ocrLanguages: new Set(),
    running: false,
    error: null,
    notice: null,
    captureWarning: null,
    developerMode: localStorage.getItem("meowcal.developerMode") === "true",
    update: { kind: "idle" },
    appVersion: null,
  };
  private engines = new EngineStatusController((patch) => this.publish(patch));
  private sessionStatus = new SessionStatusController(
    (patch) => this.publish(patch),
    () => this.snapshot,
  );
  private displays = new DisplayController((patch) => this.publish(patch));
  private updates = new UpdateController((patch) => this.publish(patch));
  private settingsWriter = new SettingsWriter((save) =>
    this.publish(settingsSaveFeedback(this.snapshot, save)),
  );

  constructor(
    private readonly subscriber: (snapshot: UiSnapshot) => void,
    private readonly clock: () => number = () => Date.now(),
  ) {}

  current(): UiSnapshot {
    return this.snapshot;
  }
  private publish(patch: Partial<UiSnapshot>): void {
    if (this.disposed) return;
    this.snapshot = { ...this.snapshot, ...patch };
    if (patch.notice && this.snapshot.busy === "idle") this.expireNotice(patch.notice);
    this.subscriber(this.snapshot);
  }

  private expireNotice(notice: string): void {
    if (this.noticeTimer !== null) window.clearTimeout(this.noticeTimer);
    this.noticeTimer = window.setTimeout(() => {
      this.noticeTimer = null;
      if (this.snapshot.notice === notice) this.publish({ notice: null });
    }, NOTICE_DURATION_MS);
  }

  dismissMessage(): void {
    this.publish({ error: null, notice: null });
  }
  async initialize(): Promise<void> {
    this.publish({ busy: "loading", error: null });
    const browserMode = window.TauriBridge.isBrowserMode();
    const [settings, languages, engine, region, running] = await Promise.all([
      this.safeInvoke<Partial<AppSettings> | null>("get_settings", null),
      this.safeInvoke<string[]>("get_ocr_languages", []),
      this.engines.read("get_engine_status", {
        phase: browserMode ? backendUnavailablePhase : "unknown",
      }),
      this.safeInvoke<CaptureRegion | null>("get_capture_region", null),
      browserMode ? false : this.safeInvoke<boolean>("is_translation_running", false),
    ]);
    const merged = mergeSettings(settings);
    this.publish({
      settingsRead: settings === null ? "error" : "ready",
      settings: merged,
      ocrLanguages: new Set(languages),
      region: browserMode ? (region ?? merged.lastCaptureRegion ?? null) : region,
      running,
      busy: "idle",
      ...(await this.updates.initialState()),
      ...(await this.displays.read()),
    });
    this.hideDiagnosticsOutsideDeveloperMode();
    await this.setupEvents();
    await this.displays.start();
    if (!browserMode) {
      const error = await this.initialSetup
        .start(engine, merged.translation.localEngine.model)
        .catch(errorMessage);
      if (error) this.publish({ error });
    }
    if (this.autoCheckTimer !== null) window.clearTimeout(this.autoCheckTimer);
    this.autoCheckTimer = window.setTimeout(() => void this.checkForUpdatesAutomatically(), 2000);
    if (!browserMode) await this.engines.finishPreparation(engine);
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
    const sessionUnlisten = await this.sessionStatus.listen();
    const wizardUnlisten = await window.TauriBridge.event.listen(
      "engine-wizard-closed",
      (event) => {
        this.initialSetup.closed(event.payload);
      },
    );
    this.unlisten.push(...sessionUnlisten, wizardUnlisten);
  }

  dispose(): void {
    this.disposed = true;
    this.engines.dispose();
    this.displays.dispose();
    this.updates.dispose();
    if (this.selectionRequest) {
      void window.TauriBridge.invoke("complete_area_selection", {
        requestId: this.selectionRequest,
        region: null,
      }).catch(() => {});
      this.selectionRequest = null;
    }
    if (this.overlaySaveId !== null) window.clearTimeout(this.overlaySaveId);
    if (this.autoCheckTimer !== null) window.clearTimeout(this.autoCheckTimer);
    if (this.noticeTimer !== null) window.clearTimeout(this.noticeTimer);
    this.unlisten.splice(0).forEach((callback) => callback());
  }

  setScreen(screen: AppScreen): void {
    this.publish({ screen, notice: null });
  }
  async setUiLanguage(value: UiLanguage): Promise<void> {
    if (this.snapshot.busy === "loading") return;
    previewUiLanguage(value);
    this.publish({ settings: { ...this.snapshot.settings, uiLanguage: value } });
    await this.settingsWriter.saveLanguage(value).catch(() => {});
  }
  async setLanguage(kind: "source" | "target", value: string): Promise<void> {
    const settings = applyLanguageSelection(structuredClone(this.snapshot.settings), kind, value);
    this.publish({ settings, notice: null });
    await this.saveSettings(true).catch(() => {});
  }
  async selectRegion(): Promise<void> {
    if (this.snapshot.busy !== "idle") return;
    if (this.snapshot.running) {
      await this.stop();
      if (!this.snapshot.running) await this.start();
      return;
    }
    if (
      (await this.chooseRegion(false)) &&
      !window.TauriBridge.isBrowserMode() &&
      this.snapshot.settingsSave.kind === "idle"
    )
      void this.engines.prewarm();
  }

  private async chooseRegion(startTranslation: boolean): Promise<boolean> {
    if (this.disposed || this.selectionRequest || this.snapshot.running) return false;
    const requestId = crypto.randomUUID();
    this.selectionRequest = requestId;
    this.publish({ busy: "selecting", error: null, notice: null });
    try {
      const result = await window.TauriBridge.invoke<{
        requestId: string;
        region: CaptureRegion | null;
        startTranslation: boolean;
      }>("request_area_selection", { requestId, startTranslation });
      if (this.disposed) return false;
      if (result.requestId !== requestId)
        throw new Error("Area selection result did not match the request.");
      this.publish({
        busy: "idle",
        ...(result.region ? { region: result.region, error: null, captureWarning: null } : {}),
      });
      return result.region !== null && (!startTranslation || result.startTranslation);
    } catch (error) {
      this.publish({ busy: "idle", error: errorMessage(error) });
      return false;
    } finally {
      this.selectionRequest = null;
    }
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
      this.publish({ busy: "idle", notice: null, error: errorMessage(error) });
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
    const revision = ++this.refreshRevision;
    const before = this.snapshot;
    const engineFallback = window.TauriBridge.isBrowserMode()
      ? { phase: backendUnavailablePhase }
      : (this.snapshot.engine ?? {});
    const [engine, region, stored, display, languages] = await Promise.all([
      this.engines.read("refresh_engine_status", engineFallback),
      this.safeInvoke<CaptureRegion | null>("get_capture_region", this.snapshot.region),
      this.safeInvoke<Partial<AppSettings> | null>("get_settings", null),
      this.displays.read(),
      before.settingsRead === "error"
        ? this.safeInvoke<string[] | null>("get_ocr_languages", null)
        : null,
    ]);
    if (revision !== this.refreshRevision || this.disposed) return;
    const patch = settingsAfterRefresh(before, this.snapshot, stored);
    if (patch.settingsRead === "ready") acceptSavedUiLanguage(patch.settings?.uiLanguage);
    if (languages) patch.ocrLanguages = new Set(languages);
    this.publish({ region, ...display, ...patch });
    await this.engines.finishPreparation(engine);
  }
  async start(useSavedArea = false): Promise<void> {
    if (
      this.disposed ||
      this.snapshot.running ||
      this.snapshot.busy !== "idle" ||
      this.selectionRequest ||
      (useSavedArea && !this.snapshot.region)
    )
      return;
    if (!useSavedArea && !(await this.chooseRegion(true))) return;
    this.sessionStatus.reset();
    this.publish({ busy: "warming", error: null, notice: null, captureWarning: null });
    try {
      await this.saveSettings(true);
      await this.engines.ready();
      if (this.disposed) return;
      this.publish({ busy: "starting" });
      await window.TauriBridge.invoke("start_translation");
      if (this.current().busy !== "starting") return;
      this.publish({ running: true, busy: "idle", notice: "Translation started" });
    } catch (error) {
      this.publish({ running: false, busy: "idle", error: errorMessage(error) });
    }
  }
  async stop(): Promise<void> {
    this.publish({ busy: "stopping", error: null });
    try {
      await window.TauriBridge.invoke("stop_translation");
      this.sessionStatus.reset();
      this.publish({ running: false, busy: "idle", notice: "Translation stopped" });
    } catch (error) {
      this.publish({ busy: "idle", error: errorMessage(error) });
    }
  }
  async saveSettings(silent = false): Promise<void> {
    if (this.snapshot.settingsRead === "error")
      throw new Error("Couldn’t load settings. Check the app connection and try again.");
    if (silent) return this.settingsWriter.save(this.snapshot.settings);
    await this.settingsWriter.retry().then(
      () => this.publish({ notice: "Settings saved" }),
      () => {}, // The writer already published the error and retained the edit.
    );
  }
  private async editSettings(edit: (settings: AppSettings) => void): Promise<void> {
    const settings = structuredClone(this.snapshot.settings);
    edit(settings);
    this.publish({ settings });
    await this.saveSettings(true).catch(() => {});
  }
  async setLogLevel(level: AppSettings["logging"]["level"]): Promise<void> {
    await this.editSettings((settings) => (settings.logging = { level }));
  }
  async setRecognitionPreset(value: keyof typeof recognitionPresets): Promise<void> {
    const preset = recognitionPresets[value];
    await this.editSettings((settings) => Object.assign(settings.translation.ocr, preset));
  }
  async setRecognition(choice: Partial<RecognitionChoice>): Promise<void> {
    await this.editSettings((settings) => applyRecognition(settings, choice));
  }
  async setTranslateAllOcrText(enabled: boolean): Promise<void> {
    await this.editSettings((settings) => (settings.translation.translateAllOcrText = enabled));
  }
  async setCpuOnly(enabled: boolean): Promise<void> {
    await this.editSettings((settings) => (settings.translation.localEngine.cpuOnly = enabled));
  }
  async setContinuity(enabled: boolean): Promise<void> {
    await this.editSettings((settings) => {
      settings.translation.enableContextAware = enabled;
      settings.translation.contextLevel = enabled ? "memoryAndRecent" : "off";
    });
  }
  async updateOverlay(patch: Partial<AppSettings["overlay"]>): Promise<void> {
    const settings = structuredClone(this.snapshot.settings);
    settings.overlay = { ...settings.overlay, ...patch };
    this.publish({ settings });
    this.settingsWriter.markPending();
    try {
      await window.TauriBridge.event.emit("overlay-settings-updated", settings.overlay);
    } catch (error) {
      this.publish({ error: errorMessage(error) });
    }
    if (this.overlaySaveId !== null) window.clearTimeout(this.overlaySaveId);
    this.overlaySaveId = window.setTimeout(() => {
      this.overlaySaveId = null;
      void this.saveSettings(true).catch(() => {});
    }, 250);
  }
  async updatePreference(
    kind: "minimizeToTray" | "autoCheckUpdates",
    enabled: boolean,
  ): Promise<void> {
    await this.editSettings((settings) => (settings[kind] = enabled));
  }
  async testTranslation(): Promise<void> {
    this.publish({ busy: "saving", notice: "Running a private sample translation…", error: null });
    try {
      await this.engines.ready();
      const result = await window.TauriBridge.invoke<WizardTestResult>("wizard_test_translation", {
        sourceText: pickSampleTranslation(this.snapshot.settings.sourceLanguage),
        sourceLanguage: this.snapshot.settings.sourceLanguage,
        targetLanguage: this.snapshot.settings.targetLanguage,
      });
      if (!result.translatedText) throw new Error("The sample translation did not return text.");
      const latency = result.latencyMs ? ` · ${result.latencyMs} ms` : "";
      this.publish({
        busy: "idle",
        notice: "Sample passed{latency}",
        noticeParameters: { latency },
      });
    } catch (error) {
      this.publish({ busy: "idle", notice: null, error: errorMessage(error) });
    }
  }

  async checkForUpdatesAutomatically(): Promise<void> {
    const completedAt = await this.updates.checkAutomatic(this.snapshot.settings, this.clock);
    if (completedAt === null) return;
    const settings = structuredClone(this.snapshot.settings);
    settings.lastUpdateCheckTimeMs = completedAt;
    this.publish({ settings });
    if (this.snapshot.settingsRead !== "ready") return;
    await window.TauriBridge.invoke("record_automatic_update_check", {
      completedAtMs: completedAt,
    }).catch(() => {});
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
    this.hideDiagnosticsOutsideDeveloperMode();
  }

  private hideDiagnosticsOutsideDeveloperMode(): void {
    if (!this.snapshot.developerMode && this.snapshot.settings.overlay.showDiagnostics) {
      void this.updateOverlay({ showDiagnostics: false });
    }
  }
}
