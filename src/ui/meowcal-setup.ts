import { LitElement, html } from "lit";
import { customElement, state } from "lit/decorators.js";
import type { AppSettings, EngineStatus } from "./contracts";
import { applyLanguageSelection, ensureDistinctLanguagePair } from "./languages";
import { pickSampleTranslation } from "./sample-translations";
import {
  activateStage,
  classifyWizardOutput,
  failCurrentStage,
  initialStages,
  type SetupStage,
} from "./setup-progress";
import { renderFooter, renderStep, type CopyState, type SampleResult } from "./setup-steps";
import "./meowcal-titlebar";

/** Asks the main window, which owns area selection, to open the selector. */
const SELECT_AREA_EVENT = "setup-select-area";

@customElement("meowcal-setup")
export class MeowcalSetup extends LitElement {
  @state() private step = 1;
  @state() private settings: AppSettings | null = null;
  @state() private ocrLanguages = new Set<string>();
  @state() private installingOcr = false;
  @state() private working = false;
  @state() private error: string | null = null;
  @state() private supportCode = "";
  @state() private sample: SampleResult | null = null;
  @state() private sampleSource = "";
  @state() private copyState: CopyState = "idle";
  @state() private stages: SetupStage[] = initialStages();
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
    try {
      const [settings, languagesAvailable] = await Promise.all([
        window.TauriBridge.invoke<AppSettings>("get_settings"),
        window.TauriBridge.invoke<string[]>("get_ocr_languages"),
      ]);
      this.settings = ensureDistinctLanguagePair(settings);
      this.ocrLanguages = new Set(languagesAvailable);
    } catch (error) {
      this.error = this.message(error);
    }
    try {
      this.unlisten.push(await window.TauriBridge.event.listen("wizard-reset", () => this.reset()));
      this.unlisten.push(
        await window.TauriBridge.event.listen("wizard-output", (event) => {
          const payload = event.payload as { line?: string; stream?: string };
          if (!payload.line) return;
          this.details = [...this.details, payload.line];
          this.advanceForLine(payload.line, payload.stream);
        }),
      );
      this.unlisten.push(
        await window.TauriBridge.event.listen("wizard-download-complete", (event) => {
          const payload = event.payload as { success?: boolean; error?: string };
          if (!payload.success) {
            this.fail(payload.error ?? "ENGINE_SETUP_FAILED");
            return;
          }
          void this.verifyReady();
        }),
      );
    } catch (error) {
      this.error = this.message(error);
    }
  }

  private message(error: unknown): string {
    return error instanceof Error ? error.message : String(error);
  }

  private sourceReady(): boolean {
    if (!this.settings) return false;
    return window.OcrLanguageTags.isOcrLanguageAvailable(
      this.ocrLanguages,
      this.settings.sourceLanguage,
    );
  }

  private async installSourceOcr(): Promise<void> {
    if (!this.settings) return;
    this.installingOcr = true;
    this.error = null;
    try {
      await window.TauriBridge.invoke("install_ocr_language", {
        languageTag: this.settings.sourceLanguage,
      });
      this.ocrLanguages = new Set(await window.TauriBridge.invoke<string[]>("get_ocr_languages"));
      if (!this.sourceReady()) {
        this.error = "Windows didn’t report the language as installed. Try again.";
      }
    } catch (error) {
      this.error = this.message(error);
    } finally {
      this.installingOcr = false;
    }
  }

  private setLanguage(kind: "source" | "target", value: string): void {
    if (!this.settings) return;
    this.settings = applyLanguageSelection(this.settings, kind, value);
  }

  private async beginEngineSetup(): Promise<void> {
    this.step = 3;
    this.working = true;
    this.error = null;
    this.supportCode = "";
    this.copyState = "idle";
    this.details = [];
    this.stages = activateStage(initialStages(), 0);
    try {
      if (this.settings) {
        await window.TauriBridge.invoke("save_settings", { settings: this.settings });
      }
      await window.TauriBridge.invoke("wizard_install_engine");
    } catch (error) {
      this.fail(this.message(error));
    }
  }

  private advanceForLine(line: string, stream?: string): void {
    const { activeStage } = classifyWizardOutput(line, stream);
    this.stages = activateStage(this.stages, activeStage);
  }

  private async verifyReady(): Promise<void> {
    try {
      this.stages = activateStage(this.stages, 3);
      await window.TauriBridge.invoke("wizard_start_service");
      const engine = await window.TauriBridge.invoke<EngineStatus>("refresh_engine_status");
      if (engine.phase !== "ready" || !engine.serviceRunning) throw new Error("ENGINE_NOT_READY");
      this.stages = activateStage(this.stages, 4);
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
      this.stages = this.stages.map((stage) => ({ ...stage, state: "complete" as const }));
      this.working = false;
      this.step = 4;
    } catch (error) {
      this.fail(this.message(error));
    }
  }

  private fail(error: string): void {
    const failure = failCurrentStage(this.stages);
    this.working = false;
    this.stages = failure.stages;
    this.error = failure.message;
    this.supportCode = error.match(/\bENGINE_[A-Z0-9_]+\b/)?.[0] ?? "ENGINE_SETUP_FAILED";
    this.details = [...this.details, error];
  }

  private reset(): void {
    this.step = 1;
    this.working = false;
    this.error = null;
    this.supportCode = "";
    this.sample = null;
    this.sampleSource = "";
    this.copyState = "idle";
    this.details = [];
    this.stages = initialStages();
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

  private async close(): Promise<boolean> {
    try {
      await window.TauriBridge.invoke("close_engine_wizard", {
        modelDownloaded: Boolean(this.sample),
        selectedModel: null,
      });
      return true;
    } catch (error) {
      this.error = this.message(error);
      return false;
    }
  }

  private async selectArea(): Promise<void> {
    if (!(await this.close())) return;
    try {
      await window.TauriBridge.event.emit(SELECT_AREA_EVENT, null);
    } catch (error) {
      this.error = this.message(error);
    }
  }

  protected render() {
    const view = {
      step: this.step,
      settings: this.settings,
      ocrReady: this.sourceReady(),
      installingOcr: this.installingOcr,
      working: this.working,
      error: this.error,
      supportCode: this.supportCode,
      stages: this.stages,
      sample: this.sample,
      sampleSource: this.sampleSource,
      copyState: this.copyState,
    };
    const actions = {
      setLanguage: (kind: "source" | "target", value: string) => this.setLanguage(kind, value),
      installOcr: () => void this.installSourceOcr(),
      prepare: () => void this.beginEngineSetup(),
      copyDetails: () => void this.copyDetails(),
      next: () => (this.step = 2),
      back: () => (this.step = 1),
      close: () => void this.close(),
      selectArea: () => void this.selectArea(),
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
