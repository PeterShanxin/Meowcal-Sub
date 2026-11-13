import { t } from "../i18n";
import { html, type TemplateResult } from "lit";
import { icon } from "./icons";
import { renderModelLicense, renderProvider } from "./model-license";

export function renderAbout(version: string | null, onBack: () => void): TemplateResult {
  return html`<main class="screen" aria-labelledby="about-title">
    <div class="page">
      <button class="quiet-button about-back" type="button" @click=${onBack}>
        ${icon("chevron-right")}${t("Settings")}
      </button>
      <header class="page-head"><h1 id="about-title" tabindex="-1">${t("About")}</h1></header>
      <div class="about-identity">
        <h2>${t("Meowcal Sub")}</h2>
        <p class="about-version">${version ?? t("Version unavailable")}</p>
        <a
          href="https://github.com/PeterShanxin/Meowcal-Sub"
          target="_blank"
          rel="noopener noreferrer"
          aria-label=${t("Meowcal Sub on GitHub")}
          >GitHub ↗</a
        >
      </div>
      <div class="about-content">${renderProvider()} ${renderModelLicense()}</div>
    </div>
  </main>`;
}
