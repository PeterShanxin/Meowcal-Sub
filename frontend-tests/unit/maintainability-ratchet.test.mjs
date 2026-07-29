import { describe, expect, it } from "vitest";
import { findRatchetRegressions } from "../../scripts/maintainability-ratchet.mjs";

const previous = {
  newProductionFileMaxLines: 400,
  eslintMaxWarnings: 10,
  frontendCoverageMinimum: {
    branches: 70,
    functions: 80,
    lines: 80,
    statements: 80,
  },
  legacyFileMaxLines: {
    "src/legacy.js": 500,
  },
};

describe("maintainability ratchet", () => {
  it("accepts ceilings that only tighten", () => {
    const current = structuredClone(previous);
    current.eslintMaxWarnings = 9;
    current.legacyFileMaxLines["src/legacy.js"] = 490;

    expect(findRatchetRegressions(current, previous)).toEqual([]);
  });

  it("rejects every supported form of baseline regression", () => {
    const current = structuredClone(previous);
    current.newProductionFileMaxLines = 401;
    current.eslintMaxWarnings = 11;
    current.frontendCoverageMinimum.lines = 79;
    current.legacyFileMaxLines["src/legacy.js"] = 501;
    current.legacyFileMaxLines["src/new-legacy.js"] = 450;

    expect(findRatchetRegressions(current, previous)).toEqual([
      "newProductionFileMaxLines increased from 400 to 401",
      "eslintMaxWarnings increased from 10 to 11",
      "frontendCoverageMinimum.lines decreased from 80 to 79",
      "src/legacy.js: legacy ceiling increased from 500 to 501",
      "src/new-legacy.js: new legacy exceptions are not allowed",
    ]);
  });
});
