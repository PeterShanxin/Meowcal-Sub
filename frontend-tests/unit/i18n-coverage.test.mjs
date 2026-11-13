import { describe, expect, it } from "vitest";
import {
  checkSource,
  makeCatalog,
  readOverlayMessages,
  validateCatalog,
} from "../../scripts/i18n-coverage.mjs";

const catalog = {
  Retry: { en: "Retry", "zh-CN": "重试" },
  "overlay.exit": { en: "Exit", "zh-CN": "退出" },
};
const check = (source) => checkSource("src/ui/example.ts", source, catalog);

describe("translation coverage gate", () => {
  it("accepts translated text and attributes, product names, endonyms and dynamic subtitle data", () => {
    expect(
      check(
        'html`<button title=${t("Retry")}>${t("Retry")}</button><p>${subtitle}</p><span>PP-OCRv6</span><option>English</option><span>${size} px</span>`',
      ),
    ).toEqual([]);
    expect(
      check(
        "html`<p>Windows OCR</p><a>GitHub ↗</a><p>Shanxin Li (@PeterShanxin)</p><option>简体中文</option>`",
      ),
    ).toEqual([]);
  });
  it("fails when a used translation is deleted, including multiline calls and no-substitution templates", () => {
    expect(check('t(\n "Missing"\n); t(`Missing too`);')).toHaveLength(2);
    expect(
      checkSource(
        "src/overlay.html",
        '<p data-i18n="missing"></p><button data-i18n-title="overlay.exit"></button>',
        catalog,
      ),
    ).toEqual(['src/overlay.html:1: missing translation key "missing"']);
  });
  it("fails when localized template text is replaced with a raw literal", () => {
    expect(check('html`<button>${t("Retry")}</button>`')).toEqual([]);
    expect(check("html`<button>Retry</button>`")).toEqual([
      'src/ui/example.ts:1: untranslated visible text "Retry"',
    ]);
    expect(check('html`<button>${"Retry"}</button>`')).toHaveLength(1);
    expect(check('html`<button>${busy ? "Wait" : "Retry"}</button>`')).toHaveLength(2);
  });
  it("checks accessibility copy without treating class names, handlers or technical logs as UI text", () => {
    expect(
      check(
        'html`<button class="primary" @click=${() => invoke("retry_command")} aria-label="Retry" title="Try again">${t("Retry")}</button>`',
      ),
    ).toHaveLength(2);
    expect(
      check(
        'console.warn("Failed to open model"); const message = "ENGINE_FAILED"; html`<div class=${active ? "on" : "off"}>${message}</div>`',
      ),
    ).toEqual([]);
    expect(check('html`<input aria-label=${busy ? "Wait" : "Retry"}>`')).toHaveLength(2);
  });
  it("ignores comments and checks nested templates independently", () => {
    expect(
      check("html`<!-- untranslated example --><div>${visible ? html`<p>Retry</p>` : null}</div>`"),
    ).toHaveLength(1);
    expect(
      checkSource(
        "src/selector.html",
        '<!-- <p data-i18n="obsolete"> -->\n<p data-i18n="overlay.exit">',
        catalog,
      ),
    ).toEqual([]);
  });
  it("requires both nonempty locales and matching named parameters", () => {
    expect(validateCatalog(catalog)).toEqual([]);
    expect(
      validateCatalog({
        incomplete: { en: "Text" },
        empty: { en: "", "zh-CN": " " },
        mismatch: { en: "Hello {name}", "zh-CN": "你好 {value}" },
      }),
    ).toHaveLength(4);
    expect(validateCatalog({ valid: { en: "{name} {count}", "zh-CN": "{count} {name}" } })).toEqual(
      [],
    );
  });
  it("reads literal catalog data without evaluating its module", () => {
    const parsed = readOverlayMessages(
      'throw new Error("must never execute"); export const overlayMessages = { "overlay.exit": { en: "Exit", "zh-CN": "退出" } } as const;',
    );
    expect(makeCatalog({ Retry: "重试" }, parsed)).toEqual(catalog);
    expect(() => makeCatalog({ "overlay.exit": "退出" }, parsed)).toThrow("Duplicate message key");
    expect(() => readOverlayMessages("const other = {};")).toThrow("literal message objects");
    expect(() => readOverlayMessages("const overlayMessages = { ...other };")).toThrow(
      "computed keys",
    );
    expect(() => readOverlayMessages('const overlayMessages = { [name]: "value" };')).toThrow(
      "computed keys",
    );
    expect(() => readOverlayMessages("const overlayMessages = { key: compute() };")).toThrow(
      "literal message objects",
    );
  });
});
