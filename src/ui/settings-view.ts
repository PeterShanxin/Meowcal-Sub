import { html, type TemplateResult } from "lit";
import type { UiSnapshot } from "./contracts";

interface SettingsActions {
  onRecognition(value: "fast" | "balanced" | "accurate"): void;
  onContinuity(enabled: boolean): void;
  onRepair(): void;
  onTest(): void;
  onDeveloper(enabled: boolean): void;
}

function recognition(snapshot: UiSnapshot): "fast" | "balanced" | "accurate" {
  const config = snapshot.settings.translation.ocr;
  if (config.enableMultiPass || config.validationStrictness === "strict") return "accurate";
  if (!config.preprocessingEnabled || config.validationStrictness === "permissive") return "fast";
  return "balanced";
}

export function renderSettings(snapshot: UiSnapshot, actions: SettingsActions): TemplateResult {
  const phase = snapshot.engine?.phase ?? "unknown";
  const engineReady = phase === "ready" || phase === "notRunning" || phase === "notrunning";
  return html`
    <main class="screen detail-screen settings-screen" aria-labelledby="settings-title">
      <header class="detail-heading">
        <span class="eyebrow"><i class="ph ph-gear" aria-hidden="true"></i> Settings</span>
        <h1 id="settings-title">Keep it simple</h1>
        <p>Everyday choices stay clear. Technical controls remain out of the way.</p>
      </header>

      <section class="settings-section" aria-labelledby="recognition-heading">
        <div class="section-heading">
          <i class="ph ph-text-aa" aria-hidden="true"></i>
          <div>
            <h2 id="recognition-heading">Recognition</h2>
            <p>Choose the balance between responsiveness and OCR effort.</p>
          </div>
        </div>
        <label class="setting-row">
          <span
            ><strong>Recognition quality</strong
            ><small>Balanced is recommended for most subtitles</small></span
          >
          <select
            .value=${recognition(snapshot)}
            @change=${(event: Event) =>
              actions.onRecognition(
                (event.target as HTMLSelectElement).value as "fast" | "balanced" | "accurate",
              )}
          >
            <option value="fast">Fast</option>
            <option value="balanced">Balanced</option>
            <option value="accurate">Accurate</option>
          </select>
        </label>
      </section>

      <section class="settings-section" aria-labelledby="translation-heading">
        <div class="section-heading">
          <i class="ph ph-translate" aria-hidden="true"></i>
          <div>
            <h2 id="translation-heading">Translation</h2>
            <p>Optional continuity can steady names across nearby lines.</p>
          </div>
        </div>
        <label class="setting-row">
          <span
            ><strong>Subtitle continuity</strong
            ><small>Uses a small source-only session memory</small></span
          >
          <input
            class="switch"
            type="checkbox"
            .checked=${snapshot.settings.translation.enableContextAware}
            @change=${(event: Event) => actions.onContinuity((event.target as HTMLInputElement).checked)}
          />
        </label>
      </section>

      <section class="settings-section" aria-labelledby="engine-heading">
        <div class="section-heading">
          <i class="ph ph-hard-drives" aria-hidden="true"></i>
          <div>
            <h2 id="engine-heading">Engine and support</h2>
            <p>HY-MT is managed and tested by Meowcal Sub.</p>
          </div>
        </div>
        <div class="setting-row engine-row">
          <span>
            <strong>Private translation engine</strong>
            <small>${engineReady ? "Installed on this PC" : "Needs attention"}</small>
          </span>
          <span class=${engineReady ? "status-chip success" : "status-chip warning"}>
            <i class="ph-fill ph-circle" aria-hidden="true"></i
            >${engineReady ? "Ready" : "Check required"}
          </span>
        </div>
        <div class="section-actions">
          <button
            class="secondary-button"
            type="button"
            @click=${actions.onTest}
            ?disabled=${snapshot.busy !== "idle"}
          >
            <i class="ph ph-check-circle" aria-hidden="true"></i> Test translation
          </button>
          <button class="secondary-button" type="button" @click=${actions.onRepair}>
            <i class="ph ph-wrench" aria-hidden="true"></i> Install or repair
          </button>
        </div>
      </section>

      <details class="advanced-panel" ?open=${snapshot.developerMode}>
        <summary>Advanced</summary>
        <label class="setting-row">
          <span
            ><strong>Developer mode</strong
            ><small>Unsupported diagnostics for development only</small></span
          >
          <input
            class="switch"
            type="checkbox"
            .checked=${snapshot.developerMode}
            @change=${(event: Event) => actions.onDeveloper((event.target as HTMLInputElement).checked)}
          />
        </label>
        ${
          snapshot.developerMode
            ? html`<div class="developer-readout">
                <span>Engine phase</span><code>${phase}</code> <span>Support code</span
                ><code>${snapshot.engine?.supportCode ?? "None"}</code>
              </div>`
            : ""
        }
      </details>
    </main>
  `;
}
