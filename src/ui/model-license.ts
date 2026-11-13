import { t } from "../i18n";
import { html } from "lit";
import license from "../../licenses/tencent-hy/License.txt?raw";
import notice from "../../licenses/tencent-hy/Notice.txt?raw";
import "../styles/model-license.css";

export function renderProvider() {
  return html`<p>
      ${t("Provided by")}
      <a href="https://github.com/PeterShanxin" target="_blank" rel="noopener noreferrer"
        >Shanxin Li (@PeterShanxin)</a
      >${t(", an individual.")}
    </p>
    <p>${t("Not affiliated with, associated with, sponsored or endorsed by Tencent.")}</p>`;
}

export function renderModelLicense() {
  return html`<section class="model-license" aria-label=${t("Translation model terms")}>
    <details>
      <summary>${t("Model license and usage terms")}</summary>
      <p><strong>${t("HY-MT1.5 translation model")}</strong></p>
      <p>
        ${t("The model license excludes use in the European Union, United Kingdom, and South Korea, including use of the model’s outputs in those territories.")}
      </p>
      <p>
        ${t("The model and its outputs are subject to the Tencent HY Community License Agreement below, including its Acceptable Use Policy and sections 5(a) and 5(b). You must comply with applicable laws and that policy. You must not use the model or its outputs to improve other AI models, except Tencent HY or its derivatives.")}
      </p>
      <p>
        ${t("These terms apply to the separately downloaded model and its outputs. Meowcal Sub’s application source remains licensed under AGPL-3.0-only.")}
      </p>
      <p>${notice}</p>
      <pre tabindex="0" aria-label=${t("Tencent HY Community License Agreement")}>${license}</pre>
    </details>
  </section>`;
}
