export type StageState = "pending" | "active" | "complete" | "error";
export type SetupStageId = "system" | "download" | "verify" | "start" | "test";

export interface SetupStage {
  id: SetupStageId;
  label: string;
  state: StageState;
}

export interface WizardOutputProgress {
  activeStage: number;
  isDiagnostic: boolean;
}

export interface SetupProgress {
  current: SetupStage;
  /** One-based position of `current`. */
  position: number;
  total: number;
  completed: number;
}

const failureMessages: Record<SetupStageId, string> = {
  system: "Setup couldn’t finish checking this PC. Try again.",
  download:
    "The download stopped. Check your internet connection and that about 1.1 GB is free, then try again.",
  verify: "The engine files didn’t pass verification. Try again.",
  start: "The engine is installed but didn’t start. Try again.",
  test: "The engine started, but the sample translation failed. Try again.",
};

export function initialStages(): SetupStage[] {
  return [
    { id: "system", label: "Checking this PC", state: "pending" },
    { id: "download", label: "Downloading engine files", state: "pending" },
    { id: "verify", label: "Verifying files", state: "pending" },
    { id: "start", label: "Starting the engine", state: "pending" },
    { id: "test", label: "Running a sample translation", state: "pending" },
  ];
}

export function classifyWizardOutput(line: string, stream?: string): WizardOutputProgress {
  const text = line.toLowerCase();
  let activeStage = 0;
  if (text.includes("download")) activeStage = 1;
  if (text.includes("verif") || text.includes("install")) activeStage = 2;
  if (text.includes("warm") || text.includes("start")) activeStage = 3;
  return { activeStage, isDiagnostic: stream === "stderr" };
}

/** Starts one stage: every stage before it has finished and none after it has begun. */
export function activateStage(stages: readonly SetupStage[], active: number): SetupStage[] {
  return stages.map((stage, index) => ({
    ...stage,
    state: index < active ? "complete" : index === active ? "active" : "pending",
  }));
}

/**
 * Fails the stage that was running and says what to do about it. Every stage
 * before it had finished, so it is shown as done; the failure copy belongs to
 * that stage rather than to setup in general.
 */
export function failCurrentStage(stages: readonly SetupStage[]): {
  stages: SetupStage[];
  message: string;
} {
  const active = stages.findIndex((stage) => stage.state === "active");
  const incomplete = stages.findIndex((stage) => stage.state !== "complete");
  const failed = active >= 0 ? active : incomplete >= 0 ? incomplete : stages.length - 1;
  return {
    stages: stages.map((stage, index) => ({
      ...stage,
      state: index < failed ? "complete" : index === failed ? "error" : "pending",
    })),
    message: failureMessages[stages[failed].id],
  };
}

/** Progress counts finished stages only; nothing here estimates time or percent. */
export function describeProgress(stages: readonly SetupStage[]): SetupProgress {
  const completed = stages.filter((stage) => stage.state === "complete").length;
  const active = stages.findIndex((stage) => stage.state === "active");
  const index = active >= 0 ? active : Math.min(completed, stages.length - 1);
  return { current: stages[index], position: index + 1, total: stages.length, completed };
}
