import { html, nothing, type TemplateResult } from "lit";
import { t } from "../i18n";
import { getUpdateNotes, type UpdateHighlight } from "./update-notes";

function inline(text: string): Array<string | TemplateResult> {
  return text.split(/(\*\*[^*\n]+\*\*|`[^`\n]+`)/).map((part) => {
    if (part.startsWith("**") && part.endsWith("**") && part.length > 4) {
      return html`<strong>${part.slice(2, -2)}</strong>`;
    }
    if (part.startsWith("`") && part.endsWith("`") && part.length > 2) {
      return html`<code>${part.slice(1, -1)}</code>`;
    }
    return part;
  });
}

function noteList(items: UpdateHighlight[]): TemplateResult {
  return html`
    <ul>
      ${items.map(
        ({ title, detail }) => html`
          <li>
            ${title ? html`<strong class="update-note-title">${inline(title)}</strong>` : nothing}
            ${detail ? inline(detail) : nothing}
          </li>
        `,
      )}
    </ul>
  `;
}

export function renderUpdateNotes(version: string, notes: string | null): TemplateResult {
  const { preview, more, fixes } = getUpdateNotes(notes);
  return html`
    <section class="update-notes" aria-label=${t("What's new")}>
      ${
        preview.length
          ? html`<h3>${t("What's new")}</h3>
              ${noteList(preview)}`
          : nothing
      }
      ${
        more.length || fixes.length
          ? html`
              <details class="update-notes-more">
                <summary>${t("More about this update")}</summary>
                ${more.length ? noteList(more) : nothing}
                ${
                  fixes.length
                    ? html`<h4>${t("Fixes and improvements")}</h4>
                        ${noteList(fixes)}`
                    : nothing
                }
              </details>
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
