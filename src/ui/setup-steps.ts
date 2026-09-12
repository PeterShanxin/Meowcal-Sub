import { html, nothing, type TemplateResult } from "lit";
import type { AppSettings, IconName } from "./contracts";
import { icon } from "./icons";
import { languageLabel, languages } from "./languages";
import { describeProgress, type SetupStage, type StageState } from "./setup-progress";

export interface SampleResult {
  translatedText?: string;
  latencyMs?: number;
}

export type CopyState = "idle" | "copied" | "failed";

export interface SetupView {
  step: number;
  settings: AppSettings | null;
  ocrReady: boolean;
  installingOcr: boolean;
  working: boolean;
  error: string | null;
  supportCode: string;
  stages: readonly SetupStage[];
  sample: SampleResult | null;
  sampleSource: string;
  copyState: CopyState;
}

export interface SetupActions {
  setLanguage(kind: "source" | "target", value: string): void;
  installOcr(): void;
  prepare(): void;
  copyDetails(): void;
  next(): void;
  back(): void;
  close(): void;
  selectArea(): void;
}

const stageWords: Record<StageState, string> = {
  pending: "Waiting",
  active: "In progress",
  complete: "Done",
  error: "Stopped",
};

const copyLabels: Record<CopyState, string> = {
  idle: "Copy details",
  copied: "Copied",
  failed: "Couldn’t copy",
};

function heading(title: string, lead: string): TemplateResult {
  return html`<h1 tabindex="-1">${title}</h1>
    <p class="wizard-lead">${lead}</p>`;
}

function errorNote(view: SetupView): TemplateResult | typeof nothing {
  return view.error
    ? html`<div class="error-box" role="alert">${icon("alert")}<span>${view.error}</span></div>`
    : nothing;
}

function benefit(name: IconName, title: string, detail: string): TemplateResult {
  return html`<div class="list-row">
    ${icon(name)}<span><strong>${title}</strong><small>${detail}</small></span>
  </div>`;
}

function stageIcon(state: StageState): TemplateResult {
  if (state === "complete") return icon("check-circle");
  if (state === "error") return icon("alert");
  if (state === "active") return icon("spinner", "spin");
  return icon("ring");
}

function languageField(
  label: string,
  value: string,
  onChange: (value: string) => void,
): TemplateResult {
  return html`<label class="field">
    <span>${label}</span>
    <select
      class="select"
      .value=${value}
      @change=${(event: Event) => onChange((event.target as HTMLSelectElement).value)}
    >
      ${languages.map(
        (language) =>
          html`<option value=${language.value} ?selected=${language.value === value}>
            ${language.label}
          </option>`,
      )}
    </select>
  </label>`;
}

function welcome(view: SetupView): TemplateResult {
  return html`<section class="wizard-content">
    ${heading("Welcome to Meowcal Sub", "Private subtitle translation for the shows you watch.")}
    <div class="wizard-body">
      <div class="list benefit-list">
        ${benefit("shield", "Everything stays on this PC", "Captured subtitles never leave your device.")}
        ${benefit("download", "One-time download, about 1.1 GB", "The translation engine and its model.")}
        ${benefit("clock", "Guided setup takes a few minutes", "Files are verified before they become active.")}
      </div>
      <p class="wizard-note">${icon("info")}Windows 11 · ARM64 or x64 · 8 GB memory</p>
      ${errorNote(view)}
    </div>
  </section>`;
}

function chooseLanguages(view: SetupView, actions: SetupActions): TemplateResult {
  if (!view.settings) {
    return html`<section class="wizard-content">
      <p class="wizard-loading">Checking Windows languages…</p>
    </section>`;
  }
  const { sourceLanguage, targetLanguage } = view.settings;
  return html`<section class="wizard-content">
    ${heading("Choose your languages", "Windows reads the original subtitles; the engine translates them.")}
    <div class="wizard-body">
      <div class="language-pair">
        ${languageField("Original subtitles", sourceLanguage, (value) => actions.setLanguage("source", value))}
        ${icon("arrow-right", "language-arrow")}
        ${languageField("Translate into", targetLanguage, (value) => actions.setLanguage("target", value))}
      </div>
      <div class="list">
        <div class="list-row">
          <span>
            <strong>${languageLabel(sourceLanguage)} text recognition</strong>
            <small>Windows may ask for permission to add it.</small>
          </span>
          <span class=${`status-chip ${view.ocrReady ? "tone-success" : "tone-warning"}`}>
            <span class="dot"></span>${view.ocrReady ? "Ready" : "Not installed"}
          </span>
        </div>
      </div>
      ${errorNote(view)}
    </div>
  </section>`;
}

function preparation(view: SetupView, actions: SetupActions): TemplateResult {
  const progress = describeProgress(view.stages);
  const failed = Boolean(view.error) && !view.working;
  return html`<section class="wizard-content">
    ${heading("Preparing translation", "Downloading, verifying, and starting the engine on this PC.")}
    <div class="wizard-body">
      ${
        failed
          ? nothing
          : html`<div class="setup-progress">
              <div class="setup-progress-label">
                <span>${progress.current.label}</span>
                <span>${progress.position} of ${progress.total}</span>
              </div>
              <div
                class="progress-track"
                role="progressbar"
                aria-label="Setup progress"
                aria-valuemin="0"
                aria-valuemax=${progress.total}
                aria-valuenow=${progress.completed}
              >
                <span style=${`width:${(progress.completed / progress.total) * 100}%`}></span>
              </div>
            </div>`
      }
      <ol class="list setup-stages" aria-live="polite">
        ${view.stages.map(
          (stage) =>
            html`<li class=${stage.state}>
              ${stageIcon(stage.state)}<span>${stage.label}</span
              ><small>${stageWords[stage.state]}</small>
            </li>`,
        )}
      </ol>
      ${
        failed
          ? html`<div class="error-box" role="alert">
              ${icon("alert")}<span>${view.error}</span>
              <div class="error-actions">
                <button class="primary-button compact" type="button" @click=${actions.prepare}>
                  ${icon("redo")}Try again
                </button>
                <button class="secondary-button" type="button" @click=${actions.copyDetails}>
                  ${icon(view.copyState === "copied" ? "check" : "copy")}${copyLabels[view.copyState]}
                </button>
                ${view.supportCode ? html`<span class="support-code">${view.supportCode}</span>` : nothing}
              </div>
            </div>`
          : nothing
      }
    </div>
  </section>`;
}

function finish(view: SetupView): TemplateResult {
  const latency = view.sample?.latencyMs;
  return html`<section class="wizard-content">
    ${icon("check-circle", "finish-mark")}
    ${heading(
      "Ready to watch",
      latency
        ? `A sample translation ran on this PC in ${latency} ms.`
        : "A sample translation ran on this PC.",
    )}
    <div class="wizard-body">
      <div class="setup-sample">
        <small>${view.sampleSource}</small><span>${view.sample?.translatedText}</span>
      </div>
      <p class="wizard-note">Next, draw a box around the subtitles of the show you’re watching.</p>
      ${errorNote(view)}
    </div>
  </section>`;
}

export function renderStep(view: SetupView, actions: SetupActions): TemplateResult {
  if (view.step === 1) return welcome(view);
  if (view.step === 2) return chooseLanguages(view, actions);
  if (view.step === 3) return preparation(view, actions);
  return finish(view);
}

function quietAction(view: SetupView, actions: SetupActions): TemplateResult {
  if (view.step === 2) {
    return html`<button class="quiet-button" type="button" @click=${actions.back}>Back</button>`;
  }
  const label =
    view.step === 4 ? "Done" : view.step === 3 && !view.working ? "Cancel setup" : "Cancel";
  return html`<button
    class="quiet-button"
    type="button"
    @click=${actions.close}
    ?disabled=${view.working}
  >
    ${label}
  </button>`;
}

function primaryAction(view: SetupView, actions: SetupActions): TemplateResult {
  if (view.step === 1) {
    return html`<button class="primary-button compact" type="button" @click=${actions.next}>
      Continue ${icon("arrow-right")}
    </button>`;
  }
  if (view.step === 2 && view.ocrReady) {
    return html`<button
      class="primary-button compact"
      type="button"
      @click=${actions.prepare}
      ?disabled=${view.installingOcr}
    >
      Prepare translation ${icon("arrow-right")}
    </button>`;
  }
  if (view.step === 2) {
    return html`<button
      class="primary-button compact"
      type="button"
      @click=${actions.installOcr}
      ?disabled=${view.installingOcr || !view.settings}
    >
      ${view.installingOcr ? icon("spinner", "spin") : icon("download")}
      ${view.installingOcr ? "Installing…" : "Install recognition"}
    </button>`;
  }
  if (view.step === 3) {
    return view.working
      ? html`<button class="primary-button compact" type="button" disabled>
          ${icon("spinner", "spin")}Setting up…
        </button>`
      : html`<span></span>`;
  }
  return html`<button class="primary-button compact" type="button" @click=${actions.selectArea}>
    ${icon("area")}Select subtitle area
  </button>`;
}

export function renderFooter(view: SetupView, actions: SetupActions): TemplateResult {
  return html`<footer class="wizard-footer">
    ${quietAction(view, actions)}
    <div class="step-indicator">
      <span>Step ${view.step} of 4</span>
      <span class="step-dots" aria-hidden="true">
        ${[1, 2, 3, 4].map(
          (value) =>
            html`<i
              class=${value === view.step ? "current" : value < view.step ? "done" : ""}
            ></i>`,
        )}
      </span>
    </div>
    ${primaryAction(view, actions)}
  </footer>`;
}
