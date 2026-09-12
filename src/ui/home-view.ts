import { html, nothing, type TemplateResult } from "lit";
import type { HomePresentation, Tone, UiSnapshot } from "./contracts";
import { icon } from "./icons";
import { languages } from "./languages";

interface HomeActions {
  onSource(value: string): void;
  onTarget(value: string): void;
  onRegion(): void;
  onPrimary(): void;
}

const stateTone: Record<HomePresentation["state"], Tone> = {
  checking: "neutral",
  notReady: "warning",
  ready: "success",
  running: "success",
  attention: "danger",
};

function languageSelect(
  label: string,
  accessibleName: string,
  value: string,
  disabled: boolean,
  onChange: (value: string) => void,
): TemplateResult {
  return html`
    <label class="field">
      <span>${label}</span>
      <select
        class="select"
        aria-label=${accessibleName}
        .value=${value}
        @change=${(event: Event) => onChange((event.target as HTMLSelectElement).value)}
        ?disabled=${disabled}
      >
        ${languages.map(
          (language) => html`
            <option value=${language.value} ?selected=${language.value === value}>
              ${language.label}
            </option>
          `,
        )}
      </select>
    </label>
  `;
}

export function renderHome(
  snapshot: UiSnapshot,
  presentation: HomePresentation,
  actions: HomeActions,
): TemplateResult {
  const tone = stateTone[presentation.state];
  const busy = presentation.actionIcon === "spinner";

  return html`
    <main class="screen home-screen" aria-labelledby="home-title">
      <div
        class=${`status-pill tone-${tone}${presentation.state === "running" ? " live" : ""}`}
        role="status"
        aria-live="polite"
      >
        ${presentation.state === "checking" ? icon("spinner", "spin") : html`<span class="dot"></span>`}
        ${presentation.statusLabel}
      </div>
      <h1 id="home-title">${presentation.title}</h1>
      <p class="home-description">${presentation.description}</p>

      <section class="session-panel" aria-label="Subtitle session">
        <div class="language-pair">
          ${languageSelect(
            "Original subtitles",
            "Original subtitle language",
            snapshot.settings.sourceLanguage,
            snapshot.running,
            actions.onSource,
          )}
          ${icon("arrow-right", "language-arrow")}
          ${languageSelect(
            "Translate into",
            "Translation language",
            snapshot.settings.targetLanguage,
            snapshot.running,
            actions.onTarget,
          )}
        </div>

        <button
          class="region-row"
          type="button"
          @click=${actions.onRegion}
          ?disabled=${snapshot.running}
        >
          <span class="region-icon">${icon("area")}</span>
          <span class="region-label">
            ${snapshot.region ? "Subtitle area selected" : "No subtitle area yet"}
          </span>
          <span class="region-action">
            ${snapshot.region ? "Change" : "Select"}${icon("chevron-right")}
          </span>
        </button>
      </section>

      <button
        class="primary-button home-action"
        type="button"
        @click=${actions.onPrimary}
        ?disabled=${presentation.actionDisabled}
      >
        ${icon(presentation.actionIcon, busy ? "spin" : "")}
        <span>${presentation.actionLabel}</span>
      </button>
      <p class=${`support-line tone-${presentation.supportTone}`}>
        ${presentation.supportTone === "neutral" ? nothing : html`<span class="dot"></span>`}
        ${presentation.supportLine}
        ${
          presentation.supportCode
            ? html`<span class="support-code">${presentation.supportCode}</span>`
            : nothing
        }
      </p>
    </main>
  `;
}
