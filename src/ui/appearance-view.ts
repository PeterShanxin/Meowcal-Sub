import { html, type TemplateResult } from "lit";
import type { OverlayConfig } from "./contracts";

interface AppearanceActions {
  onPreset(preset: "cinema" | "minimal" | "contrast"): void;
  onFontSize(value: number): void;
  onOpacity(value: number): void;
}

function opacity(config: OverlayConfig): number {
  const match = config.backgroundColor.match(/rgba?\([^,]+,[^,]+,[^,]+,?\s*([\d.]+)?\)/);
  return match?.[1] ? Math.round(Number.parseFloat(match[1]) * 100) : 72;
}

function activePreset(config: OverlayConfig): string {
  const alpha = opacity(config);
  if (config.fontSize >= 38 || alpha >= 88) return "contrast";
  if (config.fontSize <= 26 || alpha <= 35) return "minimal";
  return "cinema";
}

export function renderAppearance(
  config: OverlayConfig,
  actions: AppearanceActions,
): TemplateResult {
  const alpha = opacity(config);
  const preset = activePreset(config);
  return html`
    <main class="screen detail-screen" aria-labelledby="appearance-title">
      <header class="detail-heading">
        <span class="eyebrow"><i class="ph ph-paint-brush" aria-hidden="true"></i> Overlay</span>
        <h1 id="appearance-title">Overlay appearance</h1>
        <p>Adjust translated subtitles with an immediate, practical preview.</p>
      </header>

      <section class="preview-card" aria-label="Live subtitle preview">
        <span class="preview-label">Live preview</span>
        <div class="subtitle-preview">
          <span style=${`font-size:${config.fontSize}px;background:rgba(0,0,0,${alpha / 100})`}>
            Let’s not talk about the clock tower for now.
          </span>
        </div>
      </section>

      <section class="control-stack" aria-label="Overlay controls">
        <div class="control-row preset-row">
          <div><strong>Style preset</strong><small>Start with a tuned subtitle style</small></div>
          <div class="segmented" role="group" aria-label="Style preset">
            ${[
              ["cinema", "Cinema"],
              ["minimal", "Minimal"],
              ["contrast", "High contrast"],
            ].map(
              ([value, label]) => html`
                <button
                  type="button"
                  class=${preset === value ? "selected" : ""}
                  @click=${() => actions.onPreset(value as "cinema" | "minimal" | "contrast")}
                >
                  ${label}
                </button>
              `,
            )}
          </div>
        </div>
        <label class="control-row slider-row">
          <span
            ><strong>Font size</strong
            ><small>Keep subtitles readable at your viewing distance</small></span
          >
          <input
            type="range"
            min="18"
            max="48"
            step="1"
            .value=${String(config.fontSize)}
            @input=${(event: Event) => actions.onFontSize(Number((event.target as HTMLInputElement).value))}
          />
          <output>${config.fontSize} px</output>
        </label>
        <label class="control-row slider-row">
          <span
            ><strong>Background opacity</strong
            ><small>Balance the picture with subtitle contrast</small></span
          >
          <input
            type="range"
            min="10"
            max="100"
            step="5"
            .value=${String(alpha)}
            @input=${(event: Event) => actions.onOpacity(Number((event.target as HTMLInputElement).value))}
          />
          <output>${alpha}%</output>
        </label>
      </section>
    </main>
  `;
}
