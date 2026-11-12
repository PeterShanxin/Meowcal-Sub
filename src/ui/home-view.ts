import { html, nothing, type TemplateResult } from "lit";
import type { HomePresentation, UiSnapshot } from "./contracts";
import { icon } from "./icons";
import { isSessionLocked } from "./home-state";
import { languages } from "./languages";

interface HomeActions {
  onSource(value: string): void;
  onTarget(value: string): void;
  onRegion(): void;
  onPrimary(): void;
}

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
  const busy = presentation.actionIcon === "spinner";
  const locked = isSessionLocked(snapshot);

  return html`
    <main class="screen home-screen" aria-labelledby="home-title">
      <h1 id="home-title" tabindex="-1" aria-live="polite">${presentation.title}</h1>
      ${
        presentation.description
          ? html`<p class="home-description">${presentation.description}</p>`
          : nothing
      }

      <section class="session-panel" aria-label="Subtitle session">
        <div class="language-pair">
          ${languageSelect(
            "Original subtitles",
            "Original subtitle language",
            snapshot.settings.sourceLanguage,
            locked,
            actions.onSource,
          )}
          ${icon("arrow-right", "language-arrow")}
          ${languageSelect(
            "Translate into",
            "Translation language",
            snapshot.settings.targetLanguage,
            locked,
            actions.onTarget,
          )}
        </div>

        <button class="region-row" type="button" @click=${actions.onRegion} ?disabled=${locked}>
          <span class="region-icon">${icon("area")}</span>
          <span class="region-label">
            ${snapshot.region ? "Subtitle area selected" : "No subtitle area yet"}
          </span>
          <span class="region-action">
            ${snapshot.region ? "Change" : "Select"}${icon("chevron-right")}
          </span>
        </button>
      </section>
      ${snapshot.multipleDisplays ? html`<p class="display-notice" lang="zh-CN" role="status">${snapshot.running ? "翻译期间将保持使用已选屏幕。" : "检测到多个显示器，将使用当前窗口所在的屏幕。"}</p>` : nothing}

      <button
        class="primary-button home-action"
        type="button"
        @click=${actions.onPrimary}
        ?disabled=${presentation.actionDisabled}
      >
        ${icon(presentation.actionIcon, busy ? "spin" : "")}
        <span>${presentation.actionLabel}</span>
      </button>
      ${
        presentation.supportLine || presentation.supportCode
          ? html`<p class=${`support-line tone-${presentation.supportTone}`}>
              ${presentation.supportLine}
              ${
                presentation.supportCode
                  ? html`<span class="support-code">${presentation.supportCode}</span>`
                  : nothing
              }
            </p>`
          : nothing
      }
    </main>
  `;
}
