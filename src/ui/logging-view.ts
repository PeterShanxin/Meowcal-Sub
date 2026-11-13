import { t } from "../i18n";
import { html, type TemplateResult } from "lit";
import type { LogLevel } from "./contracts";

const descriptions: Record<LogLevel, string> = {
  off: "No new logs or captures.",
  basic: "Operational diagnostics only.",
  verbose:
    "Saves captures, OCR images and subtitles locally, including translations. Content may be private.",
};

export function renderLogging(
  level: LogLevel,
  onChange: (level: LogLevel) => void,
): TemplateResult {
  return html`
    <h2 class="group-label">${t("Local logs")}</h2>
    <div class="list">
      <label class="list-row">
        <span><strong>${t("Log level")}</strong></span>
        <select
          class="select compact"
          aria-label=${t("Log level")}
          aria-describedby="logging-privacy"
          .value=${level}
          @change=${(event: Event) => onChange((event.target as HTMLSelectElement).value as LogLevel)}
        >
          <option value="off">${t("Off")}</option>
          <option value="basic">${t("Basic")}</option>
          <option value="verbose">${t("Verbose")}</option>
        </select>
      </label>
      <p class="logging-privacy" id="logging-privacy">${t(descriptions[level])}</p>
    </div>
  `;
}
