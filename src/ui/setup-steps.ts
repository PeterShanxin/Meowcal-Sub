import { html, nothing, type TemplateResult } from "lit";
import type { AppSettings } from "./contracts";
import { icon } from "./icons";
import { renderModelLicense, renderProvider } from "./model-license";

export interface SampleResult {
  translatedText?: string;
  latencyMs?: number;
}

export type CopyState = "idle" | "copied" | "failed";

export interface SetupView {
  step: number;
  settings: AppSettings | null;
  loading: boolean;
  working: boolean;
  error: string | null;
  supportCode: string;
  progress: string;
  sample: SampleResult | null;
  sampleSource: string;
  copyState: CopyState;
  modelTermsAccepted: boolean;
  existingEngine: boolean;
}

export interface SetupActions {
  retryLoad(): void;
  prepare(): void;
  copyDetails(): void;
  next(): void;
  back(): void;
  close(): void;
  acceptModelTerms(accepted: boolean): void;
}

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

function welcome(view: SetupView): TemplateResult {
  return html`<section class="wizard-content">
    <h1 tabindex="-1">Welcome to Meowcal Sub</h1>
    <div class="wizard-body">${errorNote(view)}</div>
  </section>`;
}

function modelTerms(view: SetupView, actions: SetupActions): TemplateResult {
  if (!view.settings) {
    return html`<section class="wizard-content">
      <p class="wizard-loading">Loading settings…</p>
    </section>`;
  }
  return html`<section class="wizard-content">
    ${heading("Set up the translation model", "Review the model terms before downloading.")}
    <div class="wizard-body">
      <label class="model-consent">
        <input
          type="checkbox"
          .checked=${view.modelTermsAccepted}
          @change=${(event: Event) =>
            actions.acceptModelTerms((event.target as HTMLInputElement).checked)}
        />
        <span>I agree to the model license and usage terms.</span>
      </label>
      <div class="model-disclosure">${renderProvider()} ${renderModelLicense()}</div>
      ${errorNote(view)}
    </div>
  </section>`;
}

function preparation(view: SetupView, actions: SetupActions): TemplateResult {
  const failed = Boolean(view.error) && !view.working;
  return html`<section class="wizard-content">
    <h1 tabindex="-1">Setting up the app</h1>
    <div class="wizard-body">
      ${
        failed
          ? nothing
          : html`<p class="wizard-note" role="status">
              ${icon("spinner", "spin")}${view.progress}
            </p>`
      }
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
    ${
      view.existingEngine
        ? html`<h1 tabindex="-1">Existing setup detected</h1>`
        : heading(
            "Ready to watch",
            latency
              ? `A sample translation ran on this PC in ${latency} ms.`
              : "A sample translation ran on this PC.",
          )
    }
    <div class="wizard-body">
      ${
        view.existingEngine
          ? nothing
          : html`
              <div class="setup-sample">
                <small>${view.sampleSource}</small><span>${view.sample?.translatedText}</span>
              </div>
            `
      }
      ${errorNote(view)}
    </div>
  </section>`;
}

export function renderStep(view: SetupView, actions: SetupActions): TemplateResult {
  if (view.step === 1) return welcome(view);
  if (view.step === 2) return modelTerms(view, actions);
  if (view.step === 3) return preparation(view, actions);
  return finish(view);
}

function quietAction(view: SetupView, actions: SetupActions): TemplateResult {
  if (view.step === 4) return html`<span></span>`;
  if (view.step === 2) {
    return html`<button class="quiet-button" type="button" @click=${actions.back}>Back</button>`;
  }
  const label = view.step === 3 && !view.working ? "Cancel setup" : "Cancel";
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
    return html`<button
      class="primary-button compact"
      type="button"
      ?disabled=${view.loading}
      @click=${view.settings ? actions.next : actions.retryLoad}
    >
      ${view.loading ? "Checking this PC…" : view.settings ? "Continue" : "Try again"}
      ${icon(view.loading ? "spinner" : view.settings ? "arrow-right" : "redo", view.loading ? "spin" : "")}
    </button>`;
  }
  if (view.step === 2) {
    return html`<button
      class="primary-button compact"
      type="button"
      @click=${actions.prepare}
      ?disabled=${!view.settings || !view.modelTermsAccepted}
    >
      Set up ${icon("arrow-right")}
    </button>`;
  }
  if (view.step === 3) {
    return view.working
      ? html`<button class="primary-button compact" type="button" disabled>
          ${icon("spinner", "spin")}Setting up…
        </button>`
      : html`<span></span>`;
  }
  return html`<button class="primary-button compact" type="button" @click=${actions.close}>
    Open app ${icon("arrow-right")}
  </button>`;
}

export function renderFooter(view: SetupView, actions: SetupActions): TemplateResult {
  return html`<footer class="wizard-footer" aria-label=${`Step ${view.step} of 4`}>
    ${quietAction(view, actions)}
    ${
      view.existingEngine
        ? nothing
        : html`<div class="step-indicator">
            <span>Step ${view.step} of 4</span>
            <span class="step-dots" aria-hidden="true">
              ${[1, 2, 3, 4].map(
                (value) =>
                  html`<i
                    class=${value === view.step ? "current" : value < view.step ? "done" : ""}
                  ></i>`,
              )}
            </span>
          </div>`
    }
    ${primaryAction(view, actions)}
  </footer>`;
}
