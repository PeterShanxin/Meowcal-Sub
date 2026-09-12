import { html, nothing, type TemplateResult } from "lit";
import type { Tone, UiSnapshot } from "./contracts";
import { icon } from "./icons";
import { deriveUpdatePresentation } from "./update-state";

type Recognition = "fast" | "balanced" | "accurate";

interface SettingsActions {
  onRecognition(value: Recognition): void;
  onContinuity(enabled: boolean): void;
  onTranslateAllOcrText(enabled: boolean): void;
  onRepair(): void;
  onTest(): void;
  onDeveloper(enabled: boolean): void;
  onDiagnostics(enabled: boolean): void;
  onCheckUpdates(): void;
  onInstallUpdate(): void;
  onAutoCheckUpdates(enabled: boolean): void;
}

interface EngineRow {
  tone: Tone;
  chip: string;
  detail: string;
  actionLabel: string;
  actionIsPrimary: boolean;
  canTest: boolean;
}

function recognition(snapshot: UiSnapshot): Recognition {
  const config = snapshot.settings.translation.ocr;
  if (config.enableMultiPass || config.validationStrictness === "strict") return "accurate";
  if (!config.preprocessingEnabled || config.validationStrictness === "permissive") return "fast";
  return "balanced";
}

function engineRow(snapshot: UiSnapshot): EngineRow {
  const phase = snapshot.engine?.phase ?? "unknown";
  if (snapshot.busy === "loading" || phase === "preparing") {
    const chip = snapshot.busy === "loading" ? "Checking" : "Preparing";
    return {
      tone: "neutral",
      chip,
      detail: "Checking the engine on this PC",
      actionLabel: "Repair",
      actionIsPrimary: false,
      canTest: false,
    };
  }
  if (phase === "ready" || phase === "notRunning" || phase === "notrunning") {
    return {
      tone: "success",
      chip: "Ready",
      detail: "Installed and working",
      actionLabel: "Repair",
      actionIsPrimary: false,
      canTest: true,
    };
  }
  if (phase === "notInstalled" || phase === "notinstalled") {
    return {
      tone: "warning",
      chip: "Not installed",
      detail: "Set it up to start translating",
      actionLabel: "Set up",
      actionIsPrimary: true,
      canTest: false,
    };
  }
  return {
    tone: "danger",
    chip: "Needs repair",
    detail: "Repair it before translating",
    actionLabel: "Repair",
    actionIsPrimary: true,
    canTest: false,
  };
}

function switchRow(
  title: string,
  detail: string,
  checked: boolean,
  onChange: (checked: boolean) => void,
): TemplateResult {
  return html`
    <label class="list-row">
      <span><strong>${title}</strong><small>${detail}</small></span>
      <input
        class="switch"
        type="checkbox"
        role="switch"
        .checked=${checked}
        @change=${(event: Event) => onChange((event.target as HTMLInputElement).checked)}
      />
    </label>
  `;
}

function renderEngineAndUpdates(snapshot: UiSnapshot, actions: SettingsActions): TemplateResult {
  const engine = engineRow(snapshot);
  const update = deriveUpdatePresentation(snapshot.update, snapshot.appVersion);
  const runUpdate = update.action === "install" ? actions.onInstallUpdate : actions.onCheckUpdates;
  return html`
    <h2 class="group-label">Engine and updates</h2>
    <div class="list">
      <div class="list-row">
        <span><strong>Translation engine</strong><small>${engine.detail}</small></span>
        <span class="row-end">
          <span class=${`status-chip tone-${engine.tone}`}
            ><span class="dot"></span>${engine.chip}</span
          >
          <button
            class="secondary-button"
            type="button"
            title=${engine.canTest ? nothing : "Available once the engine is ready"}
            @click=${actions.onTest}
            ?disabled=${!engine.canTest || snapshot.busy !== "idle"}
          >
            Test
          </button>
          <button
            class=${engine.actionIsPrimary ? "primary-button compact" : "secondary-button"}
            type="button"
            @click=${actions.onRepair}
          >
            ${icon("wrench")}${engine.actionLabel}
          </button>
        </span>
      </div>
      ${switchRow(
        "Automatically check for updates",
        "At most once a day, after startup",
        snapshot.settings.autoCheckUpdates !== false,
        actions.onAutoCheckUpdates,
      )}
      <div class="list-row">
        <span><strong>${update.headline}</strong><small>${update.detail}</small></span>
        <button
          class="secondary-button"
          type="button"
          @click=${() => {
            if (update.action !== "none") runUpdate();
          }}
          ?disabled=${update.actionDisabled}
        >
          ${icon(update.action === "install" ? "download" : "update")}${update.actionLabel}
        </button>
      </div>
    </div>
    ${update.notes ? html`<pre class="update-notes">${update.notes}</pre>` : nothing}
  `;
}

function renderDeveloper(snapshot: UiSnapshot, actions: SettingsActions): TemplateResult {
  return html`
    <details class="disclosure" ?open=${snapshot.developerMode}>
      <summary>${icon("chevron-right")}Developer options</summary>
      <div class="list">
        ${switchRow(
          "Developer mode",
          "Diagnostics for development only",
          snapshot.developerMode,
          actions.onDeveloper,
        )}
        ${
          snapshot.developerMode
            ? html`
                ${switchRow(
                  "Show subtitle diagnostics",
                  "Adds recognition details beside the live subtitles",
                  snapshot.settings.overlay.showDiagnostics,
                  actions.onDiagnostics,
                )}
                <div class="list-row developer-readout">
                  <span>Engine phase</span><code>${snapshot.engine?.phase ?? "unknown"}</code>
                </div>
                <div class="list-row developer-readout">
                  <span>Support code</span><code>${snapshot.engine?.supportCode ?? "None"}</code>
                </div>
              `
            : nothing
        }
      </div>
    </details>
  `;
}

export function renderSettings(snapshot: UiSnapshot, actions: SettingsActions): TemplateResult {
  return html`
    <main class="screen" aria-labelledby="settings-title">
      <div class="page">
        <header class="page-head"><h1 id="settings-title">Settings</h1></header>

        <h2 class="group-label">Translation</h2>
        <div class="list">
          ${switchRow(
            "Translate any text",
            "Pages, apps, and games, not just subtitles",
            snapshot.settings.translation.translateAllOcrText,
            actions.onTranslateAllOcrText,
          )}
          ${switchRow(
            "Keep names consistent",
            "Remembers names and terms across nearby lines",
            snapshot.settings.translation.enableContextAware,
            actions.onContinuity,
          )}
          <label class="list-row">
            <span
              ><strong>Recognition quality</strong
              ><small>Balanced suits most subtitles</small></span
            >
            <select
              class="select compact"
              .value=${recognition(snapshot)}
              @change=${(event: Event) =>
                actions.onRecognition((event.target as HTMLSelectElement).value as Recognition)}
            >
              <option value="fast">Fast</option>
              <option value="balanced">Balanced</option>
              <option value="accurate">Accurate</option>
            </select>
          </label>
        </div>

        ${renderEngineAndUpdates(snapshot, actions)} ${renderDeveloper(snapshot, actions)}
      </div>
    </main>
  `;
}
