import { describe, expect, it } from "vitest";
import {
  classifyWizardOutput,
  describeProgress,
  failCurrentStage,
  initialStages,
  type StageState,
} from "../../src/ui/setup-progress";

function withStates(...states: StageState[]) {
  return initialStages().map((stage, index) => ({ ...stage, state: states[index] ?? "pending" }));
}

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

describe("setup progress", () => {
  // The bar used to sit at the same width with none or three of the stages
  // finished (#74). It is now the count of finished stages and nothing else.
  it("counts finished stages and names the one running", () => {
    expect(describeProgress(withStates("complete", "active"))).toMatchObject({
      current: { id: "download" },
      position: 2,
      total: 5,
      completed: 1,
    });
  });

  it("starts at the first stage with nothing finished", () => {
    expect(describeProgress(initialStages())).toMatchObject({
      current: { id: "system" },
      position: 1,
      completed: 0,
    });
  });
});

describe("setup failure", () => {
  it("fails the running stage and shows every stage before it as done", () => {
    const failure = failCurrentStage(withStates("pending", "active"));

    expect(failure.stages.map((stage) => stage.state)).toEqual([
      "complete",
      "error",
      "pending",
      "pending",
      "pending",
    ]);
    expect(failure.message).toMatch(/download stopped/i);
  });

  it("gives every stage its own recovery copy", () => {
    const messages = initialStages().map(
      (_, failed) =>
        failCurrentStage(
          initialStages().map((stage, index) => ({
            ...stage,
            state: (index < failed
              ? "complete"
              : index === failed
                ? "active"
                : "pending") as StageState,
          })),
        ).message,
    );

    expect(new Set(messages).size).toBe(5);
  });

  it("fails the first unfinished stage when none is running", () => {
    const failure = failCurrentStage(withStates("complete", "complete"));

    expect(failure.stages[2].state).toBe("error");
  });

  it("fails the last stage when every stage had finished", () => {
    const failure = failCurrentStage(
      withStates("complete", "complete", "complete", "complete", "complete"),
    );

    expect(failure.stages.at(-1)?.state).toBe("error");
    expect(failure.message).toMatch(/sample translation failed/i);
  });
});
