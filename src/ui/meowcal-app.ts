import { LocaleController } from "../i18n/lit-controller";
import { t } from "../i18n";
import { LitElement, html, nothing } from "lit";
import { customElement, state } from "lit/decorators.js";
import { AppController } from "./app-controller";
import { renderAppearance } from "./appearance-view";
import type { AppScreen, HomePresentation, IconName, UiSnapshot } from "./contracts";
import { describeError, type MessageAction, type MessagePresentation } from "./error-copy";
import { deriveHomePresentation } from "./home-state";
import { renderHome } from "./home-view";
import { icon } from "./icons";
import { profiles } from "./recognition";
import { renderSettings } from "./settings-view";
import { renderAbout } from "./about-view";
import "./meowcal-titlebar";

@customElement("meowcal-app")
export class MeowcalApp extends LitElement {
  constructor() {
    super();
    new LocaleController(this);
  }
  @state() private snapshot!: UiSnapshot;
  @state() private aboutOpen = false;
  private controller = new AppController((snapshot) => {
    this.snapshot = snapshot;
  });
  private focusRefresh = () => void this.controller.refresh();

  protected createRenderRoot(): HTMLElement | DocumentFragment {
    return this;
  }

  connectedCallback(): void {
    super.connectedCallback();
    this.snapshot = this.controller.current();
    window.addEventListener("focus", this.focusRefresh);
    void this.controller.initialize();
  }

  disconnectedCallback(): void {
    window.removeEventListener("focus", this.focusRefresh);
    this.controller.dispose();
    super.disconnectedCallback();
  }

  protected updated(changed: Map<PropertyKey, unknown>): void {
    const previous = changed.get("snapshot") as UiSnapshot | undefined;
    if ((previous && previous.screen !== this.snapshot.screen) || changed.has("aboutOpen")) {
      this.querySelector<HTMLElement>("h1")?.focus();
    }
  }

  private async runPrimary(presentation: HomePresentation): Promise<void> {
    switch (presentation.action) {
      case "setup":
      case "repair":
        await this.controller.openSetup();
        break;
      case "installOcr":
        await this.controller.installOcr();
        break;
      case "selectRegion":
        await this.controller.selectRegion();
        break;
      case "start":
        await this.controller.start();
        break;
      case "stop":
        await this.controller.stop();
        break;
      case "none":
        break;
    }
  }

  private runMessageAction(action: MessageAction): void {
    this.controller.dismissMessage();
    void (action === "selectRegion" ? this.controller.selectRegion() : this.controller.openSetup());
  }

  private renderScreen() {
    const snapshot = this.snapshot;
    if (snapshot.screen === "appearance") {
      return renderAppearance(snapshot.settings.overlay, {
        onFontSize: (fontSize) => void this.controller.updateOverlay({ fontSize }),
        onLightBackground: (lightBackground) =>
          void this.controller.updateOverlay({ lightBackground }),
      });
    }
    if (snapshot.screen === "settings") {
      if (this.aboutOpen) return renderAbout(snapshot.appVersion, () => (this.aboutOpen = false));
      return renderSettings(snapshot, {
        onUiLanguage: (value) => void this.controller.setUiLanguage(value),
        onAbout: () => (this.aboutOpen = true),
        onLogLevel: (level) => void this.controller.setLogLevel(level),
        onRecognition: (value) => void this.controller.setRecognitionPreset(value),
        onOcrEngine: (engine) => void this.controller.setRecognition({ engine }),
        onProfile: (profile) => void this.controller.setRecognition(profiles[profile]),
        onContinuity: (enabled) => void this.controller.setContinuity(enabled),
        onTranslateAllOcrText: (enabled) => void this.controller.setTranslateAllOcrText(enabled),
        onRepair: () => void this.controller.openSetup(),
        onTest: () => void this.controller.testTranslation(),
        onDeveloper: (enabled) => this.controller.setDeveloperMode(enabled),
        onDiagnostics: (showDiagnostics) => void this.controller.updateOverlay({ showDiagnostics }),
        onCheckUpdates: () => void this.controller.checkForUpdates(),
        onInstallUpdate: () => void this.controller.installUpdate(),
        onCpuOnly: (enabled) => void this.controller.setCpuOnly(enabled),
        onAutoCheckUpdates: (enabled) =>
          void this.controller.updatePreference("autoCheckUpdates", enabled),
      });
    }

    const presentation = deriveHomePresentation(snapshot);
    return renderHome(snapshot, presentation, {
      onSource: (value) => void this.controller.setLanguage("source", value),
      onTarget: (value) => void this.controller.setLanguage("target", value),
      onPrimary: () => void this.runPrimary(presentation),
      onStartSaved: () => void this.controller.start(true),
    });
  }

  private navButton(screen: AppScreen, label: string, glyph: IconName, hasIndicator = false) {
    const selected = this.snapshot.screen === screen;
    return html`
      <button
        type="button"
        class=${selected ? "nav-button selected" : "nav-button"}
        aria-current=${selected ? "page" : nothing}
        @click=${() => {
          this.aboutOpen = false;
          this.controller.setScreen(screen);
        }}
      >
        ${icon(glyph)}<span>${label}</span>
        ${
          hasIndicator
            ? html`<span
                class="nav-indicator"
                role="status"
                aria-label=${t("Update available")}
              ></span>`
            : nothing
        }
      </button>
    `;
  }

  private renderMessage() {
    const { error, notice, settingsSave } = this.snapshot;
    const unsaved = !error && settingsSave.kind === "error";
    const saving = !error && settingsSave.kind === "saving";
    if (!error && !notice && !unsaved && !saving) return nothing;
    const message: MessagePresentation = error
      ? describeError(error)
      : {
          text: unsaved
            ? t("Changes aren’t saved. Retry to keep your settings.")
            : saving
              ? t("Saving settings…")
              : t(notice ?? "", this.snapshot.noticeParameters),
          action: null,
        };
    // A notice published while work is still running describes that work.
    const working = saving || (!error && this.snapshot.busy !== "idle");
    const failed = Boolean(error || unsaved);
    const glyph = failed ? icon("alert") : working ? icon("spinner", "spin") : icon("check-circle");
    return html`
      <div class=${failed ? "toast error" : "toast notice"} role=${failed ? "alert" : "status"}>
        ${glyph}
        <span class="toast-message">${t(message.text)}</span>
        ${
          unsaved
            ? html`<button
                type="button"
                class="secondary-button"
                title=${settingsSave.kind === "error" ? describeError(settingsSave.message).text : nothing}
                @click=${() => void this.controller.saveSettings()}
              >
                ${t("Retry save")}
              </button>`
            : nothing
        }
        ${
          message.action
            ? html`<button
                type="button"
                class="link-button"
                @click=${() => this.runMessageAction(message.action!.kind)}
              >
                ${t(message.action.label)}
              </button>`
            : nothing
        }
        ${
          !unsaved && !saving
            ? html`<button
                type="button"
                class="toast-dismiss"
                aria-label=${t("Dismiss")}
                @click=${() => this.controller.dismissMessage()}
              >
                ${icon("close")}
              </button>`
            : nothing
        }
      </div>
    `;
  }

  protected render() {
    if (!this.snapshot) return nothing;
    return html`
      <div class="app-frame">
        <meowcal-titlebar label=${t("Meowcal Sub")}></meowcal-titlebar>
        ${this.renderScreen()}

        <nav class="app-nav" aria-label=${t("Main navigation")}>
          ${this.navButton("home", t("Home"), "home")}
          ${this.navButton("appearance", t("Subtitle style"), "subtitles")}
          ${this.navButton("settings", t("Settings"), "gear", this.snapshot.update.kind === "available")}
        </nav>

        ${this.renderMessage()}
      </div>
    `;
  }
}

declare global {
  interface HTMLElementTagNameMap {
    "meowcal-app": MeowcalApp;
  }
}
