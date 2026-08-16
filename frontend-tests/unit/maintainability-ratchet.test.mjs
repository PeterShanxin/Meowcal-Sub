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
  frontendCoverageScope: ["src/measured-a.js", "src/measured-b.js"],
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

describe("coverage scope", () => {
  it("rejects dropping a module from the measured scope", () => {
    const current = structuredClone(previous);
    current.frontendCoverageScope = ["src/measured-a.js"];

    expect(findRatchetRegressions(current, previous)).toEqual([
      "frontendCoverageScope no longer measures src/measured-b.js",
    ]);
  });

  it("rejects a swap that keeps the scope the same size", () => {
    const current = structuredClone(previous);
    current.frontendCoverageScope = ["src/measured-a.js", "src/measured-c.js"];
    current.frontendCoverageMinimum.lines = 70;

    expect(findRatchetRegressions(current, previous)).toEqual([
      "frontendCoverageScope no longer measures src/measured-b.js",
      "frontendCoverageMinimum.lines decreased from 80 to 70",
    ]);
  });

  it("allows a floor to fall in the change that widens the scope", () => {
    const current = structuredClone(previous);
    current.frontendCoverageScope = [...previous.frontendCoverageScope, "src/measured-c.js"];
    current.frontendCoverageMinimum.lines = 70;
    current.frontendCoverageMinimum.branches = 60;

    expect(findRatchetRegressions(current, previous)).toEqual([]);
  });

  it("still refuses a lowered floor when the scope is unchanged", () => {
    const current = structuredClone(previous);
    current.frontendCoverageMinimum.statements = 79;

    expect(findRatchetRegressions(current, previous)).toEqual([
      "frontendCoverageMinimum.statements decreased from 80 to 79",
    ]);
  });

  it("tolerates a baseline written before the scope existed", () => {
    const before = structuredClone(previous);
    delete before.frontendCoverageScope;
    const current = structuredClone(previous);

    expect(findRatchetRegressions(current, before)).toEqual([]);
  });
});
