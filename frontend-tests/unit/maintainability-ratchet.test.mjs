import { describe, expect, it } from "vitest";
import {
  MAX_COVERAGE_FLOOR_DROP,
  findRatchetRegressions,
} from "../../scripts/maintainability-ratchet.mjs";

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
      "frontendCoverageScope no longer measures src/measured-b.js, which still exists",
    ]);
  });

  it("rejects a swap that keeps the scope the same size", () => {
    const current = structuredClone(previous);
    current.frontendCoverageScope = ["src/measured-a.js", "src/measured-c.js"];
    current.frontendCoverageMinimum.lines = 70;

    expect(findRatchetRegressions(current, previous)).toEqual([
      "frontendCoverageScope no longer measures src/measured-b.js, which still exists",
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

  it("caps how far a floor may fall in one widening", () => {
    const current = structuredClone(previous);
    current.frontendCoverageScope = [...previous.frontendCoverageScope, "src/measured-c.js"];
    current.frontendCoverageMinimum.lines =
      previous.frontendCoverageMinimum.lines - MAX_COVERAGE_FLOOR_DROP - 1;

    expect(findRatchetRegressions(current, previous)).toEqual([
      `frontendCoverageMinimum.lines fell ${MAX_COVERAGE_FLOOR_DROP + 1} points ` +
        `(80 to ${current.frontendCoverageMinimum.lines}); a scope widening may lower a floor by at ` +
        `most ${MAX_COVERAGE_FLOOR_DROP}`,
    ]);
  });

  it("allows a deleted module to leave the scope", () => {
    const current = structuredClone(previous);
    current.frontendCoverageScope = ["src/measured-a.js"];
    const fileExists = (module) => module !== "src/measured-b.js";

    expect(findRatchetRegressions(current, previous, { fileExists })).toEqual([]);
  });

  it("still refuses to drop a module whose file is still there", () => {
    const current = structuredClone(previous);
    current.frontendCoverageScope = ["src/measured-a.js"];

    expect(findRatchetRegressions(current, previous, { fileExists: () => true })).toEqual([
      "frontendCoverageScope no longer measures src/measured-b.js, which still exists",
    ]);
  });

  it("does not treat a deletion on its own as growth", () => {
    const current = structuredClone(previous);
    // The file is gone, so it may leave the scope - but nothing was added, so
    // the measurement did not widen and no floor may fall on the strength of it.
    current.frontendCoverageScope = ["src/measured-a.js"];
    current.frontendCoverageMinimum.lines = 70;
    const fileExists = (module) => module !== "src/measured-b.js";

    expect(findRatchetRegressions(current, previous, { fileExists })).toEqual([
      "frontendCoverageMinimum.lines decreased from 80 to 70",
    ]);
  });

  it("tolerates a baseline written before the scope existed", () => {
    const before = structuredClone(previous);
    delete before.frontendCoverageScope;
    const current = structuredClone(previous);

    expect(findRatchetRegressions(current, before)).toEqual([]);
  });
});
