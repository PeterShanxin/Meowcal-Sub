import { describe, expect, it } from "vitest";
import { applyRecognition, profileOf, profiles } from "../../src/ui/recognition";
import { defaultSettings } from "../../src/ui/settings-defaults";

function settings() {
  return structuredClone(defaultSettings);
}

describe("recognition profiles", () => {
  it("starts new installs on the automatic profile", () => {
    expect(profileOf(settings())).toBe("automatic");
  });

  it("sets the recognizer and the translation device together", () => {
    const chosen = settings();
    applyRecognition(chosen, profiles.compatibility);
    expect(chosen.translation.ocr.engine).toBe("windows");
    expect(chosen.translation.localEngine.cpuOnly).toBe(true);
    expect(profileOf(chosen)).toBe("compatibility");
  });

  it("reports a mixed choice as custom and leaves the other choice alone", () => {
    const mixed = settings();
    applyRecognition(mixed, { cpuOnly: true });
    expect(mixed.translation.ocr.engine).toBe("ppOcrV6");
    expect(profileOf(mixed)).toBe("custom");
  });
});
