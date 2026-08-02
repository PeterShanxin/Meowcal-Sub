import { LitElement, html, nothing } from "lit";
import { customElement, state } from "lit/decorators.js";
import type { AppSettings, EngineStatus } from "./contracts";
import { applyLanguageSelection, ensureDistinctLanguagePair, languages } from "./languages";
import { pickSampleTranslation } from "./sample-translations";
import { classifyWizardOutput } from "./setup-progress";
import "./meowcal-titlebar";

const wizardLogoUrl = new URL("../assets/meowcal-icon.png", import.meta.url).href;

type Stage = "pending" | "active" | "complete" | "error";

interface SetupStage {
  id: string;
  label: string;
  state: Stage;
}

interface SampleResult {
  translatedText?: string;
  latencyMs?: number;
}

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
  @state() private details: string[] = [];
  @state() private stages: SetupStage[] = [
    { id: "system", label: "Checking this PC", state: "pending" },
    { id: "download", label: "Downloading engine files", state: "pending" },
    { id: "verify", label: "Verifying files", state: "pending" },
    { id: "start", label: "Starting the engine", state: "pending" },
    { id: "test", label: "Running a sample translation", state: "pending" },
  ];
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

  private async saveLanguages(): Promise<void> {
    if (!this.settings) return;
    await window.TauriBridge.invoke("save_settings", { settings: this.settings });
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
        this.error = "Windows did not report the language as installed. You can try again.";
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
    this.details = [];
    this.stages = this.stages.map((stage, index) => ({
      ...stage,
      state: index === 0 ? "active" : "pending",
    }));
    try {
      await this.saveLanguages();
      await window.TauriBridge.invoke("wizard_install_engine");
    } catch (error) {
      this.fail(this.message(error));
    }
  }

  private advanceForLine(line: string, stream?: string): void {
    const { activeStage } = classifyWizardOutput(line, stream);
    this.stages = this.stages.map((stage, index) => ({
      ...stage,
      state: index < activeStage ? "complete" : index === activeStage ? "active" : "pending",
    }));
  }

  private async verifyReady(): Promise<void> {
    try {
      this.setStage(3, "active");
      await window.TauriBridge.invoke("wizard_start_service");
      const engine = await window.TauriBridge.invoke<EngineStatus>("refresh_foundry_local_status");
      if (engine.phase !== "ready" || !engine.serviceRunning) throw new Error("ENGINE_NOT_READY");
      this.setStage(3, "complete");
      this.setStage(4, "active");
      if (!this.settings) throw new Error("SETTINGS_UNAVAILABLE");
      const { sourceLanguage, targetLanguage } = this.settings;
      const sample = await window.TauriBridge.invoke<SampleResult>("wizard_test_translation", {
        sourceText: pickSampleTranslation(sourceLanguage),
        sourceLanguage,
        targetLanguage,
      });
      if (!sample.translatedText) throw new Error("ENGINE_SAMPLE_TRANSLATION_FAILED");
      this.sample = sample;
      this.setStage(4, "complete");
      this.working = false;
      this.step = 4;
    } catch (error) {
      this.fail(this.message(error));
    }
  }

  private setStage(index: number, state: Stage): void {
    this.stages = this.stages.map((stage, stageIndex) =>
      stageIndex === index ? { ...stage, state } : stage,
    );
  }

  private fail(error: string): void {
    this.working = false;
    this.error =
      "Setup could not finish. Check your connection and available storage, then try again.";
    this.supportCode = error.match(/\bENGINE_[A-Z0-9_]+\b/)?.[0] ?? "ENGINE_SETUP_FAILED";
    const active = this.stages.findIndex((stage) => stage.state === "active");
    if (active >= 0) this.setStage(active, "error");
  }

  private reset(): void {
    this.step = 1;
    this.working = false;
    this.error = null;
    this.supportCode = "";
    this.sample = null;
    this.details = [];
  }

  private async close(): Promise<void> {
    try {
      await window.TauriBridge.invoke("close_foundry_wizard", {
        modelDownloaded: Boolean(this.sample),
        selectedModel: null,
      });
    } catch (error) {
      this.fail(this.message(error));
    }
  }

  private renderWelcome() {
    return html`
      <section class="wizard-content welcome-step">
        <span class="wizard-eyebrow">Setup · Step 1 of 4</span>
        <h1 tabindex="-1">Welcome to Meowcal Sub</h1>
        <p class="wizard-lead">
          Private local subtitle translation for watching shows in your language.
        </p>
        <div class="setup-benefits">
          <div>
            <i class="ph ph-shield-check" aria-hidden="true"></i
            ><span
              ><strong>Everything stays on this PC</strong
              ><small>Captured subtitles never leave your device.</small></span
            >
          </div>
          <div>
            <i class="ph ph-download-simple" aria-hidden="true"></i
            ><span
              ><strong>One-time download about 1.1 GB</strong
              ><small>The supported engine and model are included.</small></span
            >
          </div>
          <div>
            <i class="ph ph-clock" aria-hidden="true"></i
            ><span
              ><strong>Guided setup takes a few minutes</strong
              ><small>Files are verified before they become active.</small></span
            >
          </div>
        </div>
        <p class="compatibility">
          <i class="ph ph-info" aria-hidden="true"></i> Windows 11 · ARM64 or x64 · 8 GB RAM
        </p>
      </section>
    `;
  }

  private renderLanguages() {
    if (!this.settings) return html`<div class="wizard-loading">Checking Windows languages…</div>`;
    const ready = this.sourceReady();
    return html`
      <section class="wizard-content language-step">
        <span class="wizard-eyebrow">Setup · Step 2 of 4</span>
        <h1 tabindex="-1">Choose your languages</h1>
        <p class="wizard-lead">
          We’ll prepare Windows recognition and local translation for this pair.
        </p>
        <div class="wizard-language-pair">
          <label
            ><span>Original subtitles</span
            ><select
              .value=${this.settings.sourceLanguage}
              @change=${(event: Event) => this.setLanguage("source", (event.target as HTMLSelectElement).value)}
            >
              ${languages.map((language) => html`<option value=${language.value} ?selected=${language.value === this.settings?.sourceLanguage}>${language.label}</option>`)}
            </select></label
          >
          <i class="ph ph-arrow-right" aria-hidden="true"></i>
          <label
            ><span>Translate into</span
            ><select
              .value=${this.settings.targetLanguage}
              @change=${(event: Event) => this.setLanguage("target", (event.target as HTMLSelectElement).value)}
            >
              ${languages.map((language) => html`<option value=${language.value} ?selected=${language.value === this.settings?.targetLanguage}>${language.label}</option>`)}
            </select></label
          >
        </div>
        <div class="ocr-readiness">
          <div>
            <span
              ><i class="ph ph-text-aa" aria-hidden="true"></i> Selected language recognition</span
            ><strong class=${ready ? "ready" : "missing"}
              >${ready ? "Ready" : "Needs install"}</strong
            >
          </div>
          <p>Windows may ask for permission to add the recognition language.</p>
          ${ready ? nothing : html`<button class="secondary-button" type="button" @click=${() => this.installSourceOcr()} ?disabled=${this.installingOcr}><i class="ph ph-download-simple" aria-hidden="true"></i>${this.installingOcr ? "Installing…" : "Install selected OCR"}</button>`}
        </div>
      </section>
    `;
  }

  private renderProgress() {
    return html`
      <section class="wizard-content progress-step">
        <span class="wizard-eyebrow">Setup · Step 3 of 4</span>
        <h1 tabindex="-1">Preparing local translation</h1>
        <p class="wizard-lead">
          Meowcal Sub is downloading, verifying, and starting the private engine.
        </p>
        <div class="progress-track" aria-hidden="true"><span></span></div>
        <ol class="setup-stages" aria-live="polite">
          ${this.stages.map((stage) => html`<li class=${stage.state}><i class=${stage.state === "complete" ? "ph-fill ph-check-circle" : stage.state === "error" ? "ph ph-warning-circle" : stage.state === "active" ? "ph ph-spinner-gap" : "ph ph-circle"} aria-hidden="true"></i><span>${stage.label}</span><small>${stage.state === "active" ? "Working…" : stage.state}</small></li>`)}
        </ol>
        ${
          this.error
            ? html`<details class="setup-details">
                <summary>Setup details</summary>
                <pre>${this.details.join("\n") || "No additional setup output was reported."}</pre>
              </details>`
            : nothing
        }
      </section>
    `;
  }

  private renderFinish() {
    return html`
      <section class="wizard-content finish-step">
        <span class="wizard-eyebrow">Setup · Step 4 of 4</span>
        <h1 tabindex="-1">Ready to watch</h1>
        <p class="wizard-lead">A real sample translation passed on this PC.</p>
        <div class="setup-success">
          <i class="ph-fill ph-check-circle" aria-hidden="true"></i>
          <div>
            <strong>Private translation is ready</strong>
            <p>${this.sample?.translatedText}</p>
            <small>${this.sample?.latencyMs ?? "—"} ms sample latency</small>
          </div>
        </div>
        <p class="next-step">
          <i class="ph ph-selection" aria-hidden="true"></i> Close setup, select the subtitle area,
          then start translation.
        </p>
      </section>
    `;
  }

  protected render() {
    const ready = this.sourceReady();
    return html`
      <div class="wizard-frame">
        <meowcal-titlebar label="Local Translation Setup" no-maximize></meowcal-titlebar>
        <header class="wizard-brand">
          <img src=${wizardLogoUrl} alt="" /><span>Meowcal Sub</span
          ><span class="step-count">${this.step} / 4</span>
        </header>
        ${this.step === 1 ? this.renderWelcome() : this.step === 2 ? this.renderLanguages() : this.step === 3 ? this.renderProgress() : this.renderFinish()}
        <div class="wizard-error-slot">
          ${this.error ? html`<div class="wizard-error" role="alert"><i class="ph ph-warning-circle" aria-hidden="true"></i><span>${this.error}${this.supportCode ? html`<small>Support code · ${this.supportCode}</small>` : nothing}</span></div>` : nothing}
        </div>
        <footer class="wizard-footer">
          <button
            class="quiet-button"
            type="button"
            @click=${() => (this.step > 1 && this.step < 3 ? (this.step -= 1) : this.close())}
            ?disabled=${this.working}
          >
            ${this.step > 1 && this.step < 3 ? "Back" : "Cancel"}
          </button>
          <div class="step-dots" aria-label=${`Step ${this.step} of 4`}>
            ${[1, 2, 3, 4].map((value) => html`<i class=${value === this.step ? "ph-fill ph-circle active" : value < this.step ? "ph-fill ph-check-circle done" : "ph-fill ph-circle"} aria-hidden="true"></i>`)}
          </div>
          ${this.step === 1 ? html`<button class="wizard-primary" type="button" @click=${() => (this.step = 2)}>Continue <i class="ph ph-arrow-right" aria-hidden="true"></i></button>` : this.step === 2 ? html`<button class="wizard-primary" type="button" @click=${() => this.beginEngineSetup()} ?disabled=${!ready || this.installingOcr}>Prepare local translation <i class="ph ph-arrow-right" aria-hidden="true"></i></button>` : this.step === 3 ? html`<button class="wizard-primary" type="button" @click=${() => this.beginEngineSetup()} ?disabled=${this.working}>${this.working ? "Setting up…" : "Try again"}</button>` : html`<button class="wizard-primary" type="button" @click=${() => this.close()}>Finish <i class="ph ph-check" aria-hidden="true"></i></button>`}
        </footer>
      </div>
    `;
  }
}

declare global {
  interface HTMLElementTagNameMap {
    "meowcal-setup": MeowcalSetup;
  }
}
