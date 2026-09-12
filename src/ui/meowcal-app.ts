import { LitElement, html, nothing } from "lit";
import { customElement, state } from "lit/decorators.js";
import { AppController } from "./app-controller";
import { renderAppearance } from "./appearance-view";
import type { AppScreen, HomePresentation, IconName, UiSnapshot } from "./contracts";
import { describeError, type MessageAction, type MessagePresentation } from "./error-copy";
import { deriveHomePresentation } from "./home-state";
import { renderHome } from "./home-view";
import { icon } from "./icons";
import { renderSettings } from "./settings-view";
import "./meowcal-titlebar";

@customElement("meowcal-app")
export class MeowcalApp extends LitElement {
  @state() private snapshot!: UiSnapshot;
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
      return renderSettings(snapshot, {
        onRecognition: (value) => void this.controller.setRecognitionPreset(value),
        onContinuity: (enabled) => void this.controller.setContinuity(enabled),
        onTranslateAllOcrText: (enabled) => void this.controller.setTranslateAllOcrText(enabled),
        onRepair: () => void this.controller.openSetup(),
        onTest: () => void this.controller.testTranslation(),
        onDeveloper: (enabled) => this.controller.setDeveloperMode(enabled),
        onDiagnostics: (showDiagnostics) => void this.controller.updateOverlay({ showDiagnostics }),
        onCheckUpdates: () => void this.controller.checkForUpdates(),
        onInstallUpdate: () => void this.controller.installUpdate(),
        onAutoCheckUpdates: (enabled) =>
          void this.controller.updatePreference("autoCheckUpdates", enabled),
      });
    }

    const presentation = deriveHomePresentation(snapshot);
    return renderHome(snapshot, presentation, {
      onSource: (value) => void this.controller.setLanguage("source", value),
      onTarget: (value) => void this.controller.setLanguage("target", value),
      onRegion: () => void this.controller.selectRegion(),
      onPrimary: () => void this.runPrimary(presentation),
    });
  }

  private navButton(screen: AppScreen, label: string, glyph: IconName, hasIndicator = false) {
    const selected = this.snapshot.screen === screen;
    return html`
      <button
        type="button"
        class=${selected ? "nav-button selected" : "nav-button"}
        aria-current=${selected ? "page" : nothing}
        @click=${() => this.controller.setScreen(screen)}
      >
        ${icon(glyph)}<span>${label}</span>
        ${
          hasIndicator
            ? html`<span class="nav-indicator" role="status" aria-label="Update available"></span>`
            : nothing
        }
      </button>
    `;
  }

  private renderMessage() {
    const { error, notice } = this.snapshot;
    if (!error && !notice) return nothing;
    const message: MessagePresentation = error
      ? describeError(error)
      : { text: notice ?? "", action: null };
    return html`
      <div class=${error ? "toast error" : "toast notice"} role=${error ? "alert" : "status"}>
        ${icon(error ? "alert" : "check-circle")}
        <span class="toast-message">${message.text}</span>
        ${
          message.action
            ? html`<button
                type="button"
                class="link-button"
                @click=${() => this.runMessageAction(message.action!.kind)}
              >
                ${message.action.label}
              </button>`
            : nothing
        }
        <button
          type="button"
          class="toast-dismiss"
          aria-label="Dismiss"
          @click=${() => this.controller.dismissMessage()}
        >
          ${icon("close")}
        </button>
      </div>
    `;
  }

  protected render() {
    if (!this.snapshot) return nothing;
    return html`
      <div class="app-frame">
        <meowcal-titlebar label="Meowcal Sub"></meowcal-titlebar>
        ${this.renderScreen()}

        <nav class="app-nav" aria-label="Main navigation">
          ${this.navButton("home", "Home", "home")}
          ${this.navButton("appearance", "Subtitle style", "subtitles")}
          ${this.navButton("settings", "Settings", "gear", this.snapshot.update.kind === "available")}
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
