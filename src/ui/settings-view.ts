import { html, nothing, type TemplateResult } from "lit";
import type { OcrEngine, Tone, UiSnapshot } from "./contracts";
import { icon } from "./icons";
import { backendUnavailablePhase } from "./home-state";
import { deriveUpdatePresentation } from "./update-state";
import { summarizeUpdateNotes } from "./update-notes";
import { renderModelLicense } from "./model-license";
import { profileOf, type Profile } from "./recognition";

type Recognition = "fast" | "balanced" | "accurate";

interface SettingsActions {
  onRecognition(value: Recognition): void;
  onOcrEngine(engine: OcrEngine): void;
  onProfile(profile: Profile): void;
  onContinuity(enabled: boolean): void;
  onTranslateAllOcrText(enabled: boolean): void;
  onRepair(): void;
  onTest(): void;
  onDeveloper(enabled: boolean): void;
  onDiagnostics(enabled: boolean): void;
  onCheckUpdates(): void;
  onInstallUpdate(): void;
  onAutoCheckUpdates(enabled: boolean): void;
  onCpuOnly(enabled: boolean): void;
}

interface EngineRow {
  tone: Tone;
  chip: string;
  detail: string;
  actionLabel: string | null;
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
  if (snapshot.busy === "loading" || phase === "preparing" || phase === "busy") {
    const chip = snapshot.busy === "loading" ? "Checking" : phase === "busy" ? "Busy" : "Preparing";
    return {
      tone: "neutral",
      chip,
      detail:
        phase === "busy"
          ? "Waiting for the current engine operation"
          : "Checking the engine on this PC",
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
  if (phase === backendUnavailablePhase) {
    return {
      tone: "danger",
      chip: "Backend offline",
      detail: "Start the browser backend to check the engine",
      actionLabel: null,
      actionIsPrimary: false,
      canTest: false,
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
          ${
            engine.actionLabel
              ? html`<button
                  class=${engine.actionIsPrimary ? "primary-button compact" : "secondary-button"}
                  type="button"
                  @click=${actions.onRepair}
                  ?disabled=${snapshot.busy !== "idle" || ["preparing", "busy"].includes(snapshot.engine?.phase ?? "")}
                >
                  ${icon("wrench")}${engine.actionLabel}
                </button>`
              : nothing
          }
        </span>
      </div>
      ${
        snapshot.update.kind === "store"
          ? nothing
          : switchRow(
              "Automatically check for updates",
              "At most once a day, after startup",
              snapshot.settings.autoCheckUpdates !== false,
              actions.onAutoCheckUpdates,
            )
      }
      <div class="list-row">
        <span><strong>${update.headline}</strong><small>${update.detail}</small></span>
        ${
          snapshot.update.kind === "store"
            ? nothing
            : html`<button
                class="secondary-button"
                type="button"
                @click=${() => {
                  if (update.action !== "none") runUpdate();
                }}
                ?disabled=${update.actionDisabled}
              >
                ${icon(update.action === "install" ? "download" : "update")}${update.actionLabel}
              </button>`
        }
      </div>
    </div>
    ${snapshot.update.kind === "available" ? renderUpdateNotes(snapshot.update.version, update.notes) : nothing}
  `;
}

function renderUpdateNotes(version: string, notes: string | null): TemplateResult {
  const highlights = summarizeUpdateNotes(notes);
  return html`
    <section class="update-notes" aria-label="What's new">
      ${
        highlights.length
          ? html`
              <h3>What's new</h3>
              <ul>
                ${highlights.map(({ title, detail }) => html`<li>${title ? html`<strong>${title}</strong> ` : nothing}${detail}</li>`)}
              </ul>
            `
          : nothing
      }
      <a
        href=${`https://github.com/PeterShanxin/Meowcal-Sub/releases/tag/v${encodeURIComponent(version)}`}
        target="_blank"
        rel="noopener noreferrer"
        >Full release notes ↗</a
      >
    </section>
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

const profileDetail: Record<Profile | "custom", string> = {
  automatic: "PP-OCRv6 reads subtitles; translation uses the graphics card when it can",
  compatibility: "Windows OCR and CPU translation, for graphics drivers that misbehave",
  custom: "Set under Advanced",
};

const koreanNote = "Korean subtitles always use Windows OCR, because PP-OCRv6 cannot read Korean.";

function renderProfile(snapshot: UiSnapshot, actions: SettingsActions): TemplateResult {
  const profile = profileOf(snapshot.settings);
  return html`
    <label class="list-row">
      <span
        ><strong
          >Profile
          <span class="info-hint" role="img" aria-label=${koreanNote} title=${koreanNote}
            >${icon("info")}</span
          ></strong
        ><small>${profileDetail[profile]}</small></span
      >
      <select
        class="select compact"
        @change=${(event: Event) =>
          actions.onProfile((event.target as HTMLSelectElement).value as Profile)}
      >
        <option value="automatic" .selected=${profile === "automatic"}>Automatic</option>
        <option value="compatibility" .selected=${profile === "compatibility"}>
          Compatibility
        </option>
        ${profile === "custom" ? html`<option value="custom" disabled .selected=${true}>Custom</option>` : nothing}
      </select>
    </label>
  `;
}

function renderAdvanced(snapshot: UiSnapshot, actions: SettingsActions): TemplateResult {
  const ocr = snapshot.settings.translation.ocr;
  return html`
    <details class="disclosure">
      <summary>${icon("chevron-right")}Advanced</summary>
      <div class="list">
        <label class="list-row">
          <span
            ><strong>Text recognition</strong
            ><small>PP-OCRv6 reads subtitles more steadily</small></span
          >
          <select
            class="select compact"
            .value=${ocr.engine}
            @change=${(event: Event) =>
              actions.onOcrEngine((event.target as HTMLSelectElement).value as OcrEngine)}
          >
            <option value="ppOcrV6">PP-OCRv6</option>
            <option value="windows">Windows OCR</option>
          </select>
        </label>
        ${switchRow(
          "Run the engine on CPU only",
          "Try this if translations come out garbled or your PC stalls",
          snapshot.settings.translation.localEngine.cpuOnly,
          actions.onCpuOnly,
        )}
        ${
          ocr.engine === "windows"
            ? html`<label class="list-row">
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
              </label>`
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
        <header class="page-head"><h1 id="settings-title" tabindex="-1">Settings</h1></header>

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
          ${renderProfile(snapshot, actions)}
        </div>
        ${renderAdvanced(snapshot, actions)} ${renderEngineAndUpdates(snapshot, actions)}
        ${renderModelLicense()} ${renderDeveloper(snapshot, actions)}
      </div>
    </main>
  `;
}
