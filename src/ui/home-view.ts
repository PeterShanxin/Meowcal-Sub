import { html, type TemplateResult } from "lit";
import type { HomePresentation, UiSnapshot } from "./contracts";
import { languageLabel, languages } from "./languages";

interface HomeActions {
  onSource(value: string): void;
  onTarget(value: string): void;
  onRegion(): void;
  onPrimary(): void;
}

export function renderHome(
  snapshot: UiSnapshot,
  presentation: HomePresentation,
  actions: HomeActions,
): TemplateResult {
  const regionLabel = snapshot.region ? "Area selected" : "No subtitle area selected";
  const regionAction = snapshot.region ? "Change" : "Select";

  return html`
    <main class="screen home-screen" aria-labelledby="home-title">
      <section class="hero-block">
        <div class="state-pill state-${presentation.state}" role="status" aria-live="polite">
          <i class="ph-fill ph-circle" aria-hidden="true"></i>
          ${presentation.statusLabel}
        </div>
        <h1 id="home-title">${presentation.title}</h1>
        <p>${presentation.description}</p>
      </section>

      <section class="session-panel" aria-label="Subtitle session">
        <div class="language-pair">
          <label>
            <span>Original subtitles</span>
            <select
              aria-label="Original subtitle language"
              .value=${snapshot.settings.sourceLanguage}
              @change=${(event: Event) => actions.onSource((event.target as HTMLSelectElement).value)}
              ?disabled=${snapshot.running}
            >
              ${languages.map(
                (language) => html`
                  <option
                    value=${language.value}
                    ?selected=${language.value === snapshot.settings.sourceLanguage}
                  >
                    ${language.label}
                  </option>
                `,
              )}
            </select>
          </label>
          <i class="ph ph-arrow-right language-arrow" aria-hidden="true"></i>
          <label>
            <span>Translate into</span>
            <select
              aria-label="Translation language"
              .value=${snapshot.settings.targetLanguage}
              @change=${(event: Event) => actions.onTarget((event.target as HTMLSelectElement).value)}
              ?disabled=${snapshot.running}
            >
              ${languages.map(
                (language) => html`
                  <option
                    value=${language.value}
                    ?selected=${language.value === snapshot.settings.targetLanguage}
                  >
                    ${language.label}
                  </option>
                `,
              )}
            </select>
          </label>
        </div>

        <button
          class="region-row"
          type="button"
          @click=${actions.onRegion}
          ?disabled=${snapshot.running}
        >
          <span class="region-icon"><i class="ph ph-selection" aria-hidden="true"></i></span>
          <span class="region-copy">
            <strong>${regionLabel}</strong>
            <small
              >${snapshot.region ? "Original subtitle capture area" : "Draw around the subtitles on screen"}</small
            >
          </span>
          <span class="region-action"
            >${regionAction}<i class="ph ph-caret-right" aria-hidden="true"></i
          ></span>
        </button>
      </section>

      <section class="primary-zone">
        <button
          class="primary-action"
          type="button"
          @click=${actions.onPrimary}
          ?disabled=${presentation.actionDisabled}
        >
          <i class=${presentation.actionIcon} aria-hidden="true"></i>
          <span>${presentation.actionLabel}</span>
        </button>
        <div class="support-line tone-${presentation.supportTone}">
          <i class="ph-fill ph-circle" aria-hidden="true"></i>
          ${presentation.supportLine}
        </div>
      </section>

      <p class="session-summary" aria-label="Current language pair">
        ${languageLabel(snapshot.settings.sourceLanguage)}
        <i class="ph ph-arrow-right" aria-hidden="true"></i>
        ${languageLabel(snapshot.settings.targetLanguage)}
      </p>
    </main>
  `;
}
