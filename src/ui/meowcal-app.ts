import { LitElement, html, nothing } from "lit";
import { customElement, state } from "lit/decorators.js";
import { AppController } from "./app-controller";
import { renderAppearance } from "./appearance-view";
import type { AppScreen, HomePresentation, UiSnapshot } from "./contracts";
import { deriveHomePresentation } from "./home-state";
import { renderHome } from "./home-view";
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

  private renderScreen() {
    const snapshot = this.snapshot;
    if (snapshot.screen === "appearance") {
      return renderAppearance(snapshot.settings.overlay, {
        onPreset: (preset) => {
          const values = {
            cinema: { fontSize: 32, backgroundColor: "rgba(0, 0, 0, 0.72)" },
            minimal: { fontSize: 25, backgroundColor: "rgba(0, 0, 0, 0.3)" },
            contrast: { fontSize: 40, backgroundColor: "rgba(0, 0, 0, 0.94)" },
          };
          void this.controller.updateOverlay(values[preset]);
        },
        onFontSize: (fontSize) => void this.controller.updateOverlay({ fontSize }),
        onOpacity: (value) =>
          void this.controller.updateOverlay({ backgroundColor: `rgba(0, 0, 0, ${value / 100})` }),
      });
    }
    if (snapshot.screen === "settings") {
      return renderSettings(snapshot, {
        onRecognition: (value) => void this.controller.setRecognitionPreset(value),
        onContinuity: (enabled) => void this.controller.setContinuity(enabled),
        onRepair: () => void this.controller.openSetup(),
        onTest: () => void this.controller.testTranslation(),
        onDeveloper: (enabled) => this.controller.setDeveloperMode(enabled),
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

  private navButton(screen: AppScreen, label: string, icon: string) {
    const selected = this.snapshot.screen === screen;
    return html`
      <button
        type="button"
        class=${selected ? "nav-button selected" : "nav-button"}
        aria-current=${selected ? "page" : nothing}
        @click=${() => this.controller.setScreen(screen)}
      >
        <i class=${`ph ${icon}`} aria-hidden="true"></i><span>${label}</span>
      </button>
    `;
  }

  protected render() {
    if (!this.snapshot) return nothing;
    return html`
      <div class="app-frame">
        <meowcal-titlebar label="Meowcal Sub"></meowcal-titlebar>
        ${this.renderScreen()}

        <nav class="app-nav" aria-label="Main navigation">
          ${this.navButton("home", "Home", "ph-house")}
          ${this.navButton(
            "appearance",
            this.snapshot.running ? "Adjust overlay" : "Overlay appearance",
            "ph-paint-brush",
          )}
          ${this.navButton("settings", "Settings", "ph-gear")}
        </nav>

        ${
          this.snapshot.error
            ? html`<div class="toast error" role="alert">
                <i class="ph ph-warning-circle" aria-hidden="true"></i>
                <span>${this.snapshot.error}</span>
              </div>`
            : this.snapshot.notice
              ? html`<div class="toast notice" role="status">
                  <i class="ph ph-check-circle" aria-hidden="true"></i>
                  <span>${this.snapshot.notice}</span>
                </div>`
              : nothing
        }
      </div>
    `;
  }
}

declare global {
  interface HTMLElementTagNameMap {
    "meowcal-app": MeowcalApp;
  }
}
