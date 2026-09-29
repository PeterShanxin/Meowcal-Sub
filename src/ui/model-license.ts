import { html } from "lit";
import license from "../../licenses/tencent-hy/License.txt?raw";
import notice from "../../licenses/tencent-hy/Notice.txt?raw";
import "../styles/model-license.css";

export function renderModelLicense() {
  return html`<section class="model-license" aria-label="Translation model terms">
    <p><strong>HY-MT1.5 translation model</strong></p>
    <p>
      Meowcal Sub is provided by Shanxin Li, an individual. Tencent is not affiliated with,
      associated with, sponsoring, or endorsing Meowcal Sub.
    </p>
    <p>
      The model license excludes use in the European Union, United Kingdom, and South Korea,
      including use of the model’s outputs in those territories.
    </p>
    <details>
      <summary>Read model license and use restrictions</summary>
      <p>
        The model and its outputs are subject to the Tencent HY Community License Agreement below,
        including its Acceptable Use Policy and sections 5(a) and 5(b). You must comply with
        applicable laws and that policy. You must not use the model or its outputs to improve other
        AI models, except Tencent HY or its derivatives.
      </p>
      <p>
        These terms apply to the separately downloaded model and its outputs. Meowcal Sub’s
        application source remains licensed under AGPL-3.0-only.
      </p>
      <p>${notice}</p>
      <pre tabindex="0" aria-label="Tencent HY Community License Agreement">${license}</pre>
    </details>
  </section>`;
}
