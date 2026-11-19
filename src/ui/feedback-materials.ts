import { html, nothing } from "lit";
import { t } from "../i18n";
import type { FeedbackMaterial, FeedbackPreview, FeedbackType } from "./feedback-model";
import { icon } from "./icons";

const stages: Record<string, string> = {
  capture: "Capture",
  ocr: "Text recognition",
  translation: "Translation",
  overlay: "Subtitle overlay",
  engine: "Translation engine",
  app: "Application",
};
const statuses: Record<string, string> = {
  running: "In progress",
  ok: "Completed",
  failed: "Failed",
  empty: "No result",
  skipped: "Skipped",
  sent: "Sent to overlay",
};
const advice: Partial<Record<FeedbackType, string>> = {
  translation: "The original line and its translation help us check the wording.",
  ocr: "The captured region and recognized text help us check which characters were missed.",
  subtitles: "Logs near the missing subtitle can help locate the interruption.",
  error: "Logs from the time of the error can help locate the failure.",
};
const recommendations: Partial<Record<FeedbackType, readonly string[]>> = {
  translation: ["translation"],
  ocr: ["screenshot", "ocr"],
  subtitles: ["log"],
  error: ["log"],
};

export function renderDiagnostics(preview: FeedbackPreview, fault: boolean, logLevel: string) {
  return html`<details class="feedback-diagnostics" ?open=${fault || !preview.stages.length}>
    <summary>
      ${icon("clock")}<span>${t("Basic diagnostics")}</span
      ><small>${t(fault ? "At the error" : "Latest session")}</small>
    </summary>
    <dl>
      <div>
        <dt>${t("App version")}</dt>
        <dd>${preview.version}</dd>
      </div>
      ${preview.stages.map(
        (stage) =>
          html`<div>
            <dt>${t(stages[stage.stage] ?? stage.stage)}</dt>
            <dd>
              ${t(statuses[stage.status] ?? stage.status)}
              ${stage.elapsedMs === null ? nothing : t("{time} ms", { time: stage.elapsedMs })}
              ${stage.code ? html`<code>${stage.code}</code>` : nothing}
            </dd>
          </div>`,
      )}
    </dl>
    ${!preview.stages.length ? html`<p>${t(logLevel === "off" ? "Logging is off. Send a description now, or choose Basic in Settings and reproduce the issue." : "No session diagnostics are available. You can still send a description.")}</p>` : nothing}
  </details>`;
}

export function materialAdvice(materials: FeedbackMaterial[], type: FeedbackType | "") {
  if (
    !type ||
    !advice[type] ||
    !materials.some((item) => recommendations[type]?.includes(item.kind))
  )
    return nothing;
  return html`<p class="feedback-advice">${t(advice[type]!)}</p>`;
}

export function renderMaterials(
  materials: FeedbackMaterial[],
  type: FeedbackType | "",
  selected: ReadonlySet<string>,
  onChange: (id: string, checked: boolean) => void,
) {
  const kinds = type ? (recommendations[type] ?? []) : [];
  const recommended = (item: FeedbackMaterial) => kinds.includes(item.kind);
  const sorted = [...materials].sort((a, b) => Number(recommended(b)) - Number(recommended(a)));
  return html`<div class="feedback-materials">
    ${sorted.map(
      (item) =>
        html`<div class="feedback-material">
          <label
            ><input
              type="checkbox"
              .checked=${selected.has(item.id)}
              @change=${(event: Event) => onChange(item.id, (event.target as HTMLInputElement).checked)}
            /><span>${t(item.label)}</span></label
          >
          ${item.text ? html`<pre>${item.text}</pre>` : nothing}
          ${item.imageDataUrl?.startsWith("data:image/png;base64,") ? html`<img src=${item.imageDataUrl} alt=${t("Captured region preview")} />` : nothing}
        </div>`,
    )}
  </div>`;
}
