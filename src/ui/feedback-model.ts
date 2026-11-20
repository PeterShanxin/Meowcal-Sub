import type { UiSnapshot } from "./contracts";

export const feedbackTypes = {
  translation: "Translation quality",
  ocr: "Text recognition issue",
  subtitles: "Missing subtitles",
  error: "Application error",
  suggestion: "Feature suggestion",
  other: "Other",
} as const;
export type FeedbackType = keyof typeof feedbackTypes;
export type MaterialKind = "translation" | "ocr" | "screenshot" | "log" | "path";

export interface FeedbackMaterial {
  id: string;
  kind: MaterialKind;
  label: string;
  text?: string | null;
  imageDataUrl?: string | null;
}

export interface FeedbackPreview {
  snapshotId: string;
  version: string;
  stages: Array<{ stage: string; status: string; elapsedMs: number | null; code: string | null }>;
  materials: FeedbackMaterial[];
}

export type PreviewResult = Promise<FeedbackPreview | null>;

export function requestPreview(faultCode: string | null, faultToken?: string): PreviewResult {
  return window.TauriBridge.invoke<FeedbackPreview>("feedback_preview", {
    faultCode,
    faultToken,
  }).catch(() => null);
}

export function currentFault(
  snapshot: UiSnapshot,
): { code: string; identity: string; token?: string } | null {
  if (snapshot.error)
    return {
      code:
        snapshot.captureFault?.message === snapshot.error
          ? snapshot.captureFault.code
          : "APP_ERROR",
      identity: snapshot.error,
      token:
        snapshot.captureFault?.message === snapshot.error ? snapshot.captureFault.token : undefined,
    };
  if (snapshot.settingsSave.kind === "error")
    return { code: "APP_ERROR", identity: snapshot.settingsSave.message };
  if (snapshot.captureWarning) return { code: "APP_ERROR", identity: snapshot.captureWarning };
  const engine = snapshot.engine;
  if (
    engine &&
    ["error", "noModels", "nomodels", "damaged", "invalid"].includes(engine.phase ?? "")
  ) {
    const code = /^ENGINE_[A-Z0-9_]{1,64}$/.test(engine.supportCode ?? "")
      ? engine.supportCode!
      : "APP_ERROR";
    return { code, identity: code };
  }
  return null;
}
