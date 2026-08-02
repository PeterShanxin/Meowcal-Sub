import { LitElement, html, svg, type TemplateResult } from "lit";
import { customElement, property, state } from "lit/decorators.js";

// Inline so the controls stay crisp at 10px and do not depend on the icon font.
const GLYPH = {
  minimize: svg`<path d="M1 6h10" />`,
  maximize: svg`<rect x="1.5" y="1.5" width="9" height="9" rx="1" />`,
  restore: svg`<rect x="1.5" y="3.5" width="7" height="7" rx="1" /><path d="M4 3.5V2a.5.5 0 0 1 .5-.5H10a.5.5 0 0 1 .5.5v5.5a.5.5 0 0 1-.5.5H8.5" />`,
  close: svg`<path d="M2 2l8 8M10 2l-8 8" />`,
};

function icon(glyph: TemplateResult<2>): TemplateResult<2> {
  return svg`<svg viewBox="0 0 12 12" width="12" height="12" fill="none"
    stroke="currentColor" stroke-width="1.2" stroke-linecap="round" aria-hidden="true">${glyph}</svg>`;
}

/**
 * Window chrome for the undecorated shell windows.
 *
 * The Windows title bar is disabled so the dark app frame is not capped by a
 * light system strip. That makes the window controls this component's job:
 * without them an undecorated window cannot be minimised, maximised, or closed.
 *
 * Rendered into light DOM so the shared app-shell styles apply.
 */
@customElement("meowcal-titlebar")
export class MeowcalTitlebar extends LitElement {
  @property({ type: String }) label = "Meowcal Sub";

  /** Fixed-size windows hide the maximise control instead of showing a dead button. */
  @property({ type: Boolean, attribute: "no-maximize" }) noMaximize = false;

  @state() private maximized = false;

  protected createRenderRoot(): HTMLElement | DocumentFragment {
    return this;
  }

  connectedCallback(): void {
    super.connectedCallback();
    void this.syncMaximized();
  }

  private async syncMaximized(): Promise<void> {
    this.maximized = (await window.TauriBridge?.windowControls?.isMaximized()) === true;
  }

  private async run(action: "minimize" | "toggleMaximize" | "close"): Promise<void> {
    const controls = window.TauriBridge?.windowControls;
    if (!controls) return;
    await controls[action]();
    if (action === "toggleMaximize") await this.syncMaximized();
  }

  protected render(): TemplateResult {
    return html`
      <div class="titlebar" data-tauri-drag-region>
        <div class="titlebar-identity" data-tauri-drag-region>
          <img src="./assets/meowcal-icon.png" alt="" aria-hidden="true" />
          <span data-tauri-drag-region>${this.label}</span>
        </div>
        <div class="titlebar-controls">
          <button
            type="button"
            class="titlebar-button"
            aria-label="Minimize"
            @click=${() => this.run("minimize")}
          >
            ${icon(GLYPH.minimize)}
          </button>
          ${
            this.noMaximize
              ? null
              : html`<button
                  type="button"
                  class="titlebar-button"
                  aria-label=${this.maximized ? "Restore" : "Maximize"}
                  @click=${() => this.run("toggleMaximize")}
                >
                  ${icon(this.maximized ? GLYPH.restore : GLYPH.maximize)}
                </button>`
          }
          <button
            type="button"
            class="titlebar-button titlebar-close"
            aria-label="Close"
            @click=${() => this.run("close")}
          >
            ${icon(GLYPH.close)}
          </button>
        </div>
      </div>
    `;
  }
}

declare global {
  interface HTMLElementTagNameMap {
    "meowcal-titlebar": MeowcalTitlebar;
  }
}
