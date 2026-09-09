import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

const read = (relativePath) =>
  readFileSync(fileURLToPath(new URL(relativePath, import.meta.url)), "utf8");

const configRs = read("../../src-tauri/src/config.rs");
const settingsDefaults = read("../../src/ui/settings-defaults.ts");

const rustDefault = (field) => {
  const match = configRs.match(new RegExp(`${field}:\\s*(\\d+)`));
  return match ? Number(match[1]) : null;
};

const frontendDefault = (field) => {
  const match = settingsDefaults.match(new RegExp(`${field}:\\s*(\\d+),`));
  return match ? Number(match[1]) : null;
};

describe("settings defaults", () => {
  // `saveSettings` posts the whole settings object back to Rust, so the
  // frontend's copy of a default overwrites the backend's whenever anything is
  // saved. A capture interval the backend had already lowered to 250ms kept
  // returning to 500ms this way, and there is no UI control to correct it by
  // hand - it added up to half a second of pure detection delay per line.
  it("agrees with the backend on the capture interval", () => {
    const backend = rustDefault("capture_interval_ms");

    expect(backend).not.toBeNull();
    expect(frontendDefault("captureIntervalMs")).toBe(backend);
  });

  // Same failure mode as the capture interval, with a worse result: a frontend
  // default of `true` would post general-text translation back over a viewer's
  // stored `false` the first time anything else was saved.
  it("agrees with the backend that translating all OCR text is off by default", () => {
    expect(configRs).toMatch(/translate_all_ocr_text: false,/);
    expect(settingsDefaults).toMatch(/translateAllOcrText: false,/);
  });
});
