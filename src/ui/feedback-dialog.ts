import { html, LitElement, nothing } from "lit";
import { customElement, property, state } from "lit/decorators.js";
import { t } from "../i18n";
import { LocaleController } from "../i18n/lit-controller";
import {
  feedbackTypes,
  type FeedbackType,
  type FeedbackPreview,
  type PreviewResult,
} from "./feedback-model";
import { materialAdvice, renderDiagnostics, renderMaterials } from "./feedback-materials";
import { icon } from "./icons";

const sendErrors: Record<string, string> = {
  FEEDBACK_NOT_CONFIGURED: "Feedback delivery is unavailable in this build.",
  FEEDBACK_TOO_LARGE: "The selected materials are too large. Remove an item and send again.",
  FEEDBACK_RATE_LIMITED: "Too many reports were sent recently. Please try again later.",
  FEEDBACK_OFFLINE: "The service did not confirm delivery. Check your connection and send again.",
  FEEDBACK_PENDING: "The service did not confirm delivery. Check your connection and send again.",
};

@customElement("meowcal-feedback")
export class FeedbackDialog extends LitElement {
  @property({ attribute: false }) logLevel = "basic";
  @state() private type: FeedbackType | "" = "";
  @state() private description = "";
  @state() private preview: FeedbackPreview | null = null;
  @state() private loading = false;
  @state() private fault = false;
  @state() private expanded = false;
  @state() private selected = new Set<string>();
  @state() private sending = false;
  @state() private issue = 0;
  @state() private message = "";
  private requestId = "";
  private generation = 0;

  constructor() {
    super();
    new LocaleController(this);
  }
  protected createRenderRoot() {
    return this;
  }

  async open(faultCode: string | null, preview: PreviewResult): Promise<void> {
    const generation = ++this.generation;
    this.fault = faultCode !== null;
    this.type = faultCode?.startsWith("OCR_") ? "ocr" : this.fault ? "error" : "";
    this.description = "";
    this.selected = new Set();
    this.expanded = false;
    this.sending = false;
    this.issue = 0;
    this.message = "";
    this.preview = null;
    this.requestId = crypto.randomUUID();
    this.loading = true;
    await this.updateComplete;
    if (generation !== this.generation) return;
    const dialog = this.querySelector("dialog")!;
    if (!dialog.open) dialog.showModal();
    const result = await preview;
    if (generation !== this.generation) return;
    this.loading = false;
    this.preview = result;
  }

  private close() {
    if (!this.sending) this.querySelector("dialog")?.close();
  }

  private clearPreview() {
    ++this.generation;
    this.description = "";
    this.preview = null;
    this.selected = new Set();
    this.dispatchEvent(new Event("feedback-closed"));
  }

  private edited() {
    this.message = "";
    this.requestId = crypto.randomUUID();
  }

  private selectMaterial(id: string, checked: boolean) {
    const selected = new Set(this.selected);
    if (checked) selected.add(id);
    else selected.delete(id);
    this.selected = selected;
    this.edited();
  }

  private async send(event: SubmitEvent) {
    event.preventDefault();
    if (!this.type || !this.preview || this.sending || this.issue) return;
    if (!this.fault && !this.description.trim()) {
      const input = this.querySelector("textarea")!;
      input.setCustomValidity(t("Describe what happened before sending."));
      input.reportValidity();
      return;
    }
    this.sending = true;
    this.message = "";
    const generation = this.generation;
    try {
      const { issueNumber } = await window.TauriBridge.invoke<{ issueNumber: number }>(
        "feedback_send",
        {
          request: {
            requestId: this.requestId,
            snapshotId: this.preview.snapshotId,
            type: this.type,
            description: this.description,
            materialIds: [...this.selected],
          },
        },
      );
      if (!Number.isSafeInteger(issueNumber) || issueNumber < 1) throw Error("FEEDBACK_PENDING");
      if (this.generation === generation) {
        this.issue = issueNumber;
        this.preview = null;
        this.description = "";
        this.selected = new Set();
      }
    } catch (error) {
      const code = error instanceof Error ? error.message : String(error);
      this.message = sendErrors[code] ?? "Feedback could not be sent. Your draft is still here.";
    } finally {
      this.sending = false;
    }
  }

  protected render() {
    const preview = this.preview;
    return html`<dialog
      class="feedback-dialog"
      aria-labelledby="feedback-heading"
      @close=${this.clearPreview}
      @cancel=${(e: Event) => {
        if (this.sending) e.preventDefault();
      }}
    >
      <header class="feedback-header">
        <span class="feedback-header-icon">${icon("feedback")}</span>
        <h2 id="feedback-heading">${t("Feedback")}</h2>
        <button
          class="toast-dismiss"
          type="button"
          title=${t("Close")}
          aria-label=${t("Close")}
          ?disabled=${this.sending}
          @click=${this.close}
        >
          ${icon("close")}
        </button>
      </header>
      ${
        this.issue
          ? html`<section class="feedback-receipt" role="status">
              ${icon("check-circle")}
              <h3>${t("Feedback sent")}</h3>
              <a
                href=${`https://github.com/PeterShanxin/Meowcal-Sub/issues/${this.issue}`}
                target="_blank"
                rel="noopener noreferrer"
                >${t("View issue #{number}", { number: this.issue })} ↗</a
              >
            </section>`
          : html`<form @submit=${this.send}>
              <div class="feedback-scroll">
                <fieldset class="feedback-fields" ?disabled=${this.sending}>
                  <label class="field"
                    ><span>${t("Feedback type")}</span
                    ><select
                      class="select"
                      required
                      .value=${this.type}
                      @change=${(e: Event) => {
                        this.type = (e.target as HTMLSelectElement).value as FeedbackType;
                        this.edited();
                      }}
                    >
                      <option value="" disabled ?selected=${!this.type}>
                        ${t("Choose a type")}
                      </option>
                      ${Object.entries(feedbackTypes).map(([key, label]) => html`<option value=${key} ?selected=${key === this.type}>${t(label)}</option>`)}
                    </select></label
                  >
                  <label class="field"
                    ><span
                      >${t("Description")}${this.fault ? html` <small>${t("Optional")}</small>` : nothing}</span
                    >
                    <textarea
                      rows="2"
                      maxlength="4000"
                      ?required=${!this.fault}
                      placeholder=${t("What happened?")}
                      .value=${this.description}
                      @input=${(e: Event) => {
                        const input = e.target as HTMLTextAreaElement;
                        input.setCustomValidity("");
                        this.description = input.value;
                        this.edited();
                      }}
                    ></textarea>
                  </label>
                  ${
                    preview
                      ? html`<section class="feedback-attachments">
                          ${renderDiagnostics(preview, this.fault, this.logLevel)}
                          ${
                            preview.materials.length
                              ? html`<div class="feedback-material-picker">
                                  <label
                                    ><span>${t("Attach troubleshooting materials")}</span
                                    ><input
                                      role="switch"
                                      type="checkbox"
                                      class="switch"
                                      .checked=${this.expanded}
                                      @change=${(e: Event) => {
                                        this.expanded = (e.target as HTMLInputElement).checked;
                                        if (!this.expanded && this.selected.size) {
                                          this.selected = new Set();
                                          this.edited();
                                        }
                                      }}
                                  /></label>
                                  ${materialAdvice(preview.materials, this.type, this.logLevel)}
                                  ${this.expanded ? renderMaterials(preview.materials, this.type, this.selected, (id, checked) => this.selectMaterial(id, checked)) : nothing}
                                </div>`
                              : materialAdvice(preview.materials, this.type, this.logLevel)
                          }
                        </section>`
                      : html`<p role="status" class="feedback-muted">
                          ${this.loading ? t("Preparing local preview…") : t("The local preview is unavailable. Reopen feedback in the Windows app.")}
                        </p>`
                  }
                </fieldset>
              </div>
              <footer class="feedback-footer">
                ${this.message ? html`<p class="feedback-error-text" role="alert">${t(this.message)}</p>` : nothing}
                <div class="feedback-footer-row">
                  <p>
                    ${icon("shield")}
                    <span>
                      ${t("Only the team maintaining the app can see your report and attachments.")}<br />
                      ${t("We automatically delete them 30 days after we close your feedback.")}
                    </span>
                  </p>
                  <button
                    class="primary-button compact feedback-send"
                    type="submit"
                    ?disabled=${!preview || this.sending}
                  >
                    ${this.sending ? icon("spinner", "spin") : nothing}${t("Send")}
                  </button>
                </div>
              </footer>
            </form>`
      }
    </dialog>`;
  }
}
