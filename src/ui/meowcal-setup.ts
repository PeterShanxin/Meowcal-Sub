import { LitElement, html } from "lit";
import { customElement, state } from "lit/decorators.js";
import type { AppSettings, EngineStatus } from "./contracts";
import { ensureDistinctLanguagePair } from "./languages";
import { pickSampleTranslation } from "./sample-translations";
import { setupProgress } from "./setup-progress";
import { renderFooter, renderStep, type CopyState, type SampleResult } from "./setup-steps";
import "./meowcal-titlebar";

@customElement("meowcal-setup")
export class MeowcalSetup extends LitElement {
  @state() private step = 1;
  @state() private settings: AppSettings | null = null;
  @state() private loading = true;
  @state() private working = false;
  @state() private error: string | null = null;
  @state() private supportCode = "";
  @state() private sample: SampleResult | null = null;
  @state() private sampleSource = "";
  @state() private copyState: CopyState = "idle";
  @state() private progress = "Preparing setup…";
  @state() private modelTermsAccepted = false;
  @state() private existingEngine = false;
  private details: string[] = [];
  private unlisten: Array<() => void> = [];

  protected createRenderRoot(): HTMLElement | DocumentFragment {
    return this;
  }

  connectedCallback(): void {
    super.connectedCallback();
    void this.initialize();
  }

  disconnectedCallback(): void {
    this.unlisten.splice(0).forEach((callback) => callback());
    super.disconnectedCallback();
  }

  protected updated(changed: Map<PropertyKey, unknown>): void {
    if (changed.has("step")) this.querySelector<HTMLElement>("h1")?.focus();
  }

  private async initialize(): Promise<void> {
    this.loading = true;
    try {
      await this.listenForSetup();
      if (!window.TauriBridge.isBrowserMode()) {
        this.reset(await window.TauriBridge.invoke<boolean>("wizard_existing_engine"));
      }
      await this.loadSettings();
    } catch (error) {
      this.unlisten.splice(0).forEach((unlisten) => unlisten());
      this.error = this.message(error);
      this.loading = false;
    }
  }

  private async loadSettings(): Promise<void> {
    if (!this.loading && this.settings) return;
    this.loading = true;
    this.error = null;
    try {
      const settings = await window.TauriBridge.invoke<AppSettings>("get_settings");
      this.settings = ensureDistinctLanguagePair(settings);
    } catch (error) {
      console.warn("[Meowcal] setup settings unavailable", error);
      this.error = "Couldn’t load settings. Check the app connection and try again.";
    } finally {
      this.loading = false;
    }
  }

  private async listenForSetup(): Promise<void> {
    this.unlisten.push(
      await window.TauriBridge.event.listen("wizard-reset", (event) =>
        this.reset(event.payload === true),
      ),
    );
    this.unlisten.push(
      await window.TauriBridge.event.listen("wizard-output", (event) => {
        const payload = event.payload as { line?: string; stream?: string };
        if (!payload.line) return;
        this.details.push(payload.line);
        if (this.details.length > 200) this.details.shift();
        this.progress = setupProgress(payload.line) ?? this.progress;
      }),
    );
  }

  private message(error: unknown): string {
    return error instanceof Error ? error.message : String(error);
  }

  private async beginEngineSetup(): Promise<void> {
    if (this.working || !this.modelTermsAccepted || !this.settings) return;
    this.step = 3;
    this.working = true;
    this.error = null;
    this.supportCode = "";
    this.copyState = "idle";
    this.details = [];
    this.progress = "Preparing setup…";
    try {
      await window.TauriBridge.invoke("save_settings", { settings: this.settings });
      await window.TauriBridge.invoke("wizard_install_engine");
      await this.verifyReady();
    } catch (error) {
      this.fail(this.message(error));
    }
  }

  private async verifyReady(): Promise<void> {
    this.progress = "Starting the engine…";
    await window.TauriBridge.invoke("wizard_start_service");
    const engine = await window.TauriBridge.invoke<EngineStatus>("refresh_engine_status");
    if (engine.phase !== "ready" || !engine.serviceRunning) throw new Error("ENGINE_NOT_READY");
    this.progress = "Running a sample translation…";
    if (!this.settings) throw new Error("SETTINGS_UNAVAILABLE");
    const { sourceLanguage, targetLanguage } = this.settings;
    const sourceText = pickSampleTranslation(sourceLanguage);
    const sample = await window.TauriBridge.invoke<SampleResult>("wizard_test_translation", {
      sourceText,
      sourceLanguage,
      targetLanguage,
    });
    if (!sample.translatedText) throw new Error("ENGINE_SAMPLE_TRANSLATION_FAILED");
    this.sample = sample;
    this.sampleSource = sourceText;
    this.working = false;
    this.step = 4;
  }

  private fail(error: string): void {
    this.working = false;
    this.error = "Setup couldn’t finish. Try again or copy the details for help.";
    this.supportCode = error.match(/\bENGINE_[A-Z0-9_]+\b/)?.[0] ?? "ENGINE_SETUP_FAILED";
    this.details.push(error);
  }

  private reset(existing = false): void {
    this.modelTermsAccepted = false;
    this.existingEngine = existing;
    this.step = existing ? 4 : 1;
    this.working = false;
    this.error = null;
    this.supportCode = "";
    this.sample = null;
    this.sampleSource = "";
    this.copyState = "idle";
    this.details = [];
    this.progress = "Preparing setup…";
  }

  private async copyDetails(): Promise<void> {
    const report = [this.supportCode, ...this.details].filter(Boolean).join("\n");
    try {
      await navigator.clipboard.writeText(report);
      this.copyState = "copied";
    } catch (error) {
      console.warn("[Meowcal] setup details could not be copied", error);
      this.copyState = "failed";
    }
  }

  private async close(): Promise<void> {
    try {
      await window.TauriBridge.invoke("close_engine_wizard", {
        modelDownloaded: Boolean(this.sample) || this.existingEngine,
        selectedModel: null,
      });
    } catch (error) {
      this.error = this.message(error);
    }
  }

  protected render() {
    const view = {
      step: this.step,
      settings: this.settings,
      loading: this.loading,
      working: this.working,
      error: this.error,
      supportCode: this.supportCode,
      progress: this.progress,
      sample: this.sample,
      sampleSource: this.sampleSource,
      copyState: this.copyState,
      modelTermsAccepted: this.modelTermsAccepted,
      existingEngine: this.existingEngine,
    };
    const actions = {
      retryLoad: () => void (this.unlisten.length ? this.loadSettings() : this.initialize()),
      prepare: () => void this.beginEngineSetup(),
      copyDetails: () => void this.copyDetails(),
      next: () => (this.step = 2),
      back: () => (this.step = 1),
      close: () => void this.close(),
      acceptModelTerms: (accepted: boolean) => (this.modelTermsAccepted = accepted),
    };
    return html`
      <div class="wizard-frame">
        <meowcal-titlebar label="Meowcal Sub Setup" no-maximize></meowcal-titlebar>
        ${renderStep(view, actions)} ${renderFooter(view, actions)}
      </div>
    `;
  }
}

declare global {
  interface HTMLElementTagNameMap {
    "meowcal-setup": MeowcalSetup;
  }
}
