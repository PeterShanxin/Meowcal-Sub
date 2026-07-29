import { createRequire } from "node:module";
import { describe, expect, it } from "vitest";

const require = createRequire(import.meta.url);
const { updateSubtitleHint } = require("../../src/scripts/overlay-subtitle-hint.js");

function hintElements() {
  const classes = new Set();
  return {
    hint: {
      classList: {
        add: (...names) => names.forEach((name) => classes.add(name)),
        remove: (...names) => names.forEach((name) => classes.delete(name)),
      },
    },
    text: { textContent: "" },
    classes,
  };
}

describe("overlay subtitle hints", () => {
  it("keeps the overlay clean when the local engine translated normally", () => {
    const elements = hintElements();
    elements.classes.add("visible");
    elements.text.textContent = "old warning";

    updateSubtitleHint(elements.hint, elements.text, "foundry_local", []);

    expect(elements.text.textContent).toBe("");
    expect(elements.classes.has("visible")).toBe(false);
  });

  it("labels OCR as fallback without leaking endpoint details", () => {
    const elements = hintElements();

    updateSubtitleHint(elements.hint, elements.text, "mock", [
      "foundry_local: timeout at http://127.0.0.1:11436",
    ]);

    expect(elements.text.textContent).toBe("Fallback: OCR · Foundry Timeout");
    expect(elements.classes.has("visible")).toBe(true);
    expect(elements.classes.has("hint-warn")).toBe(true);
    expect(elements.text.textContent).not.toContain("127.0.0.1");
  });
});
