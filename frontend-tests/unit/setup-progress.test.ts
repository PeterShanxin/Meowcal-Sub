import { describe, expect, it } from "vitest";
import { classifyWizardOutput } from "../../src/ui/setup-progress";

describe("wizard output classification", () => {
  it.each([
    ["Checking this PC", 0],
    ["Downloading engine files", 1],
    ["Installing and verifying files", 2],
    ["Warming up the service", 3],
  ])("maps %s to stage %i", (line, activeStage) => {
    expect(classifyWizardOutput(line)).toMatchObject({ activeStage, isDiagnostic: false });
  });

  it("keeps stderr as diagnostic output without classifying it as failure", () => {
    expect(classifyWizardOutput("warning: retrying download", "stderr")).toEqual({
      activeStage: 1,
      isDiagnostic: true,
    });
  });
});
