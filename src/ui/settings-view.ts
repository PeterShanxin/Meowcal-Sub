import { t, getUiLanguage, type UiLanguage } from "../i18n";
import { html, nothing, type TemplateResult } from "lit";
import type { LogLevel, OcrEngine, UiSnapshot } from "./contracts";
import { icon } from "./icons";
import { backendUnavailablePhase } from "./home-state";
import { deriveUpdatePresentation } from "./update-state";
import { summarizeUpdateNotes } from "./update-notes";
import { renderLogging } from "./logging-view";
import { profileOf, type Profile } from "./recognition";

type Recognition = "fast" | "balanced" | "accurate";

interface SettingsActions {
  onUiLanguage(value: UiLanguage): void;
  onAbout(): void;
  onLogLevel(level: LogLevel): void;
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
  chip: string;
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
    const chip =
      snapshot.busy === "loading" ? t("Checking") : phase === "busy" ? t("Busy") : t("Preparing");
    return {
      chip,
      actionLabel: t("Repair"),
      actionIsPrimary: false,
      canTest: false,
    };
  }
  if (phase === "ready" || phase === "notRunning" || phase === "notrunning") {
    return {
      chip: t("Ready"),
      actionLabel: t("Repair"),
      actionIsPrimary: false,
      canTest: true,
    };
  }
  if (phase === backendUnavailablePhase) {
    return {
      chip: t("Backend offline"),
      actionLabel: null,
      actionIsPrimary: false,
      canTest: false,
    };
  }
  if (phase === "notInstalled" || phase === "notinstalled") {
    return {
      chip: t("Not installed"),
      actionLabel: t("Set up"),
      actionIsPrimary: true,
      canTest: false,
    };
  }
  return {
    chip: t("Needs repair"),
    actionLabel: t("Repair"),
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
    <h2 class="group-label">${t("Engine and updates")}</h2>
    <div class="list">
      <div class="list-row">
        <span><strong>${t("Translation engine")}</strong><small>${engine.chip}</small></span>
        <span class="row-end">
          <button
            class="secondary-button"
            type="button"
            title=${engine.canTest ? nothing : t("Available once the engine is ready")}
            @click=${actions.onTest}
            ?disabled=${!engine.canTest || snapshot.busy !== "idle"}
          >
            ${t("Test")}
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
              t("Automatically check for updates"),
              t("At most once a day, after startup"),
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
    <section class="update-notes" aria-label=${t("What's new")}>
      ${
        highlights.length
          ? html`
              <h3>${t("What's new")}</h3>
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
        >${t("Full release notes ↗")}</a
      >
    </section>
  `;
}

function renderDeveloper(snapshot: UiSnapshot, actions: SettingsActions): TemplateResult {
  return html`
    <details class="disclosure" ?open=${snapshot.developerMode}>
      <summary>${icon("chevron-right")}${t("Developer options")}</summary>
      <div class="list">
        ${switchRow(
          t("Developer mode"),
          t("Diagnostics for development only"),
          snapshot.developerMode,
          actions.onDeveloper,
        )}
        ${
          snapshot.developerMode
            ? html`
                ${switchRow(
                  t("Show subtitle diagnostics"),
                  t("Adds recognition details beside the live subtitles"),
                  snapshot.settings.overlay.showDiagnostics,
                  actions.onDiagnostics,
                )}
                <div class="list-row developer-readout">
                  <span>${t("Engine phase")}</span
                  ><code>${snapshot.engine?.phase ?? t("Unknown")}</code>
                </div>
                <div class="list-row developer-readout">
                  <span>${t("Support code")}</span
                  ><code>${snapshot.engine?.supportCode ?? t("None")}</code>
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
          >${t("Profile")}
          <span class="info-hint" role="img" aria-label=${t(koreanNote)} title=${t(koreanNote)}
            >${icon("info")}</span
          ></strong
        ><small>${t(profileDetail[profile])}</small></span
      >
      <select
        class="select compact"
        @change=${(event: Event) =>
          actions.onProfile((event.target as HTMLSelectElement).value as Profile)}
      >
        <option value="automatic" .selected=${profile === "automatic"}>${t("Automatic")}</option>
        <option value="compatibility" .selected=${profile === "compatibility"}>
          ${t("Compatibility")}
        </option>
        ${profile === "custom" ? html`<option value="custom" disabled .selected=${true}>${t("Custom")}</option>` : nothing}
      </select>
    </label>
  `;
}

function renderAdvanced(snapshot: UiSnapshot, actions: SettingsActions): TemplateResult {
  const ocr = snapshot.settings.translation.ocr;
  return html`
    <details class="disclosure">
      <summary>${icon("chevron-right")}${t("Advanced")}</summary>
      <div class="list">
        <label class="list-row">
          <span
            ><strong>${t("Text recognition")}</strong
            ><small>${t("PP-OCRv6 reads subtitles more steadily")}</small></span
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
          t("Run the engine on CPU only"),
          t("Try this if translations come out garbled or your PC stalls"),
          snapshot.settings.translation.localEngine.cpuOnly,
          actions.onCpuOnly,
        )}
        ${
          ocr.engine === "windows"
            ? html`<label class="list-row">
                <span
                  ><strong>${t("Recognition quality")}</strong
                  ><small>${t("Balanced suits most subtitles")}</small></span
                >
                <select
                  class="select compact"
                  .value=${recognition(snapshot)}
                  @change=${(event: Event) =>
                    actions.onRecognition((event.target as HTMLSelectElement).value as Recognition)}
                >
                  <option value="fast">${t("Fast")}</option>
                  <option value="balanced">${t("Balanced")}</option>
                  <option value="accurate">${t("Accurate")}</option>
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
        <header class="page-head">
          <h1 id="settings-title" tabindex="-1">${t("Settings")}</h1>
        </header>

        <div class="list">
          <label class="list-row">
            <span
              ><strong>${t("App language")}</strong
              ><small>${t("Applies immediately. Subtitle languages stay unchanged.")}</small></span
            >
            <select
              class="select compact"
              aria-label=${t("App language")}
              ?disabled=${snapshot.busy === "loading"}
              .value=${getUiLanguage()}
              @change=${(event: Event) => actions.onUiLanguage((event.target as HTMLSelectElement).value as UiLanguage)}
            >
              <option value="system" ?selected=${getUiLanguage() === "system"}>
                ${t("System default")}
              </option>
              <option value="en" lang="en" ?selected=${getUiLanguage() === "en"}>English</option>
              <option value="zh-CN" lang="zh-CN" ?selected=${getUiLanguage() === "zh-CN"}>
                简体中文
              </option>
            </select>
          </label>
        </div>
        <h2 class="group-label">${t("Translation")}</h2>
        <div class="list">
          ${switchRow(
            t("Translate any text"),
            t("Pages, apps, and games, not just subtitles"),
            snapshot.settings.translation.translateAllOcrText,
            actions.onTranslateAllOcrText,
          )}
          ${switchRow(
            t("Keep names consistent"),
            t("Remembers names and terms across nearby lines"),
            snapshot.settings.translation.enableContextAware,
            actions.onContinuity,
          )}
          ${renderProfile(snapshot, actions)}
        </div>
        ${renderAdvanced(snapshot, actions)} ${renderEngineAndUpdates(snapshot, actions)}
        ${renderLogging(snapshot.settings.logging.level, actions.onLogLevel)}
        ${renderDeveloper(snapshot, actions)}
        <div class="list">
          <button class="list-row about-entry" type="button" @click=${actions.onAbout}>
            <strong>${t("About")}</strong>${icon("chevron-right")}
          </button>
        </div>
      </div>
    </main>
  `;
}
