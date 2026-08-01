export interface WizardOutputProgress {
  activeStage: number;
  isDiagnostic: boolean;
}

export function classifyWizardOutput(line: string, stream?: string): WizardOutputProgress {
  const text = line.toLowerCase();
  let activeStage = 0;
  if (text.includes("download")) activeStage = 1;
  if (text.includes("verif") || text.includes("install")) activeStage = 2;
  if (text.includes("warm") || text.includes("start")) activeStage = 3;
  return { activeStage, isDiagnostic: stream === "stderr" };
}
