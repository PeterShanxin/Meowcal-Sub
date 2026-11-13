import { t } from "../i18n";
import { html, type TemplateResult } from "lit";
import type { OverlayConfig } from "./contracts";

interface SubtitleStyleActions {
  onFontSize(value: number): void;
  onLightBackground(light: boolean): void;
}

export function renderAppearance(
  config: OverlayConfig,
  actions: SubtitleStyleActions,
): TemplateResult {
  const { FONT_SIZE_MIN, FONT_SIZE_MAX, clampFontSize } = window.OverlayAppearance;
  const fontSize = clampFontSize(config.fontSize);
  const ratio = (fontSize - FONT_SIZE_MIN) / (FONT_SIZE_MAX - FONT_SIZE_MIN);
  const light = config.lightBackground === true;

  return html`
    <main class="screen" aria-labelledby="appearance-title">
      <div class="page">
        <header class="page-head">
          <h1 id="appearance-title" tabindex="-1">${t("Subtitle style")}</h1>
          <p>${t("Changes apply to live subtitles right away.")}</p>
        </header>

        <section class="subtitle-preview" aria-label=${t("Subtitle preview")}>
          <span class="preview-tag">${t("Preview")}</span>
          <span
            class=${light ? "subtitle-plate light" : "subtitle-plate"}
            style=${`font-size:${fontSize}px`}
          >
            ${t("Let’s not talk about the clock tower for now.")}
          </span>
        </section>

        <section class="list" aria-label=${t("Subtitle appearance")}>
          <label class="list-row">
            <strong>${t("Text size")}</strong>
            <span class="slider-control">
              <input
                class="slider"
                type="range"
                min=${FONT_SIZE_MIN}
                max=${FONT_SIZE_MAX}
                step="1"
                style=${`--ratio:${ratio}`}
                .value=${String(fontSize)}
                @input=${(event: Event) =>
                  actions.onFontSize(Number((event.target as HTMLInputElement).value))}
              />
              <output>${fontSize} px</output>
            </span>
          </label>
          <div class="list-row">
            <span
              ><strong>${t("Background")}</strong
              ><small>${t("Light suits bright scenes")}</small></span
            >
            <div class="segmented" role="group" aria-label=${t("Subtitle background")}>
              <button
                type="button"
                aria-pressed=${String(!light)}
                @click=${() => actions.onLightBackground(false)}
              >
                ${t("Dark")}
              </button>
              <button
                type="button"
                aria-pressed=${String(light)}
                @click=${() => actions.onLightBackground(true)}
              >
                ${t("Light")}
              </button>
            </div>
          </div>
        </section>
      </div>
    </main>
  `;
}
