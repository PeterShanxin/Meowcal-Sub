import type { HomePresentation, UiSnapshot } from "./contracts";

const repairPhases = new Set(["error", "noModels", "nomodels", "damaged", "invalid"]);
const missingPhases = new Set(["notInstalled", "notinstalled"]);
const preparingPhases = new Set(["preparing"]);
const readyPhases = new Set(["ready", "notRunning", "notrunning"]);

function ocrReady(snapshot: UiSnapshot): boolean {
  return window.OcrLanguageTags.isOcrLanguageAvailable(
    snapshot.ocrLanguages,
    snapshot.settings.sourceLanguage,
  );
}

export function deriveHomePresentation(snapshot: UiSnapshot): HomePresentation {
  if (snapshot.busy === "loading") {
    return {
      state: "checking",
      statusLabel: "Checking",
      title: "Getting things ready",
      description: "Checking local translation, recognition, and your subtitle area.",
      action: "none",
      actionLabel: "Checking this PC…",
      actionIcon: "ph ph-spinner-gap",
      actionDisabled: true,
      supportLine: "Private local processing",
      supportTone: "neutral",
    };
  }

  if (snapshot.running) {
    return {
      state: "running",
      statusLabel: "Running",
      title: "Subtitles are live",
      description: "Keep watching—translated subtitles will stay above your selected area.",
      action: "stop",
      actionLabel: snapshot.busy === "stopping" ? "Stopping translation…" : "Stop translation",
      actionIcon: "ph-fill ph-stop",
      actionDisabled: snapshot.busy !== "idle",
      supportLine: "Overlay active · Local processing",
      supportTone: "success",
    };
  }

  if (snapshot.busy === "warming" || snapshot.busy === "starting") {
    return {
      state: "checking",
      statusLabel: "Starting",
      title: "Starting local translation",
      description: "The first start can take a little longer while the engine warms up.",
      action: "none",
      actionLabel: "Starting translation…",
      actionIcon: "ph ph-spinner-gap",
      actionDisabled: true,
      supportLine: "Preparing the private engine",
      supportTone: "accent",
    };
  }

  const phase = snapshot.engine?.phase ?? "unknown";
  if (missingPhases.has(phase)) {
    return {
      state: "notReady",
      statusLabel: "Not ready",
      title: "Set up private translation",
      description: "A short guided setup will download and test the supported local engine.",
      action: "setup",
      actionLabel: "Set up local translation",
      actionIcon: "ph ph-download-simple",
      actionDisabled: false,
      supportLine: "Engine not installed · Local setup required",
      supportTone: "warning",
    };
  }

  if (preparingPhases.has(phase)) {
    return {
      state: "checking",
      statusLabel: "Preparing",
      title: "Warming up local translation",
      description:
        "The private engine is checking its local model. You can start when it is ready.",
      action: "none",
      actionLabel: "Preparing engine…",
      actionIcon: "ph ph-spinner-gap",
      actionDisabled: true,
      supportLine: "Preparing the private engine",
      supportTone: "accent",
    };
  }

  if (repairPhases.has(phase) || !readyPhases.has(phase)) {
    return {
      state: "attention",
      statusLabel: "Needs attention",
      title: "Translation needs a quick repair",
      description: "Your settings are safe. Meowcal Sub can verify and restore the local engine.",
      action: "repair",
      actionLabel: "Repair translation engine",
      actionIcon: "ph ph-wrench",
      actionDisabled: false,
      supportLine: snapshot.engine?.supportCode
        ? `Support code · ${snapshot.engine.supportCode}`
        : "Engine check failed · Repair available",
      supportTone: "danger",
    };
  }

  if (!ocrReady(snapshot)) {
    return {
      state: "notReady",
      statusLabel: "Almost ready",
      title: "Install the selected language",
      description: "Windows needs the matching recognition language before it can read subtitles.",
      action: "installOcr",
      actionLabel: "Install required OCR",
      actionIcon: "ph ph-text-aa",
      actionDisabled: snapshot.busy !== "idle",
      supportLine: "Recognition language missing · Windows installation required",
      supportTone: "warning",
    };
  }

  if (!snapshot.region) {
    return {
      state: "notReady",
      statusLabel: "Almost ready",
      title: "Choose the subtitle area",
      description: "Draw a box around the original subtitles once, then start watching.",
      action: "selectRegion",
      actionLabel: "Select subtitle area",
      actionIcon: "ph ph-selection",
      actionDisabled: false,
      supportLine: "Engine ready · Area not selected",
      supportTone: "warning",
    };
  }

  return {
    state: "ready",
    statusLabel: "Ready",
    title: "Ready for subtitles",
    description: "Start when your episode is playing. Everything stays on this PC.",
    action: "start",
    actionLabel: "Start translation",
    actionIcon: "ph-fill ph-play",
    actionDisabled: snapshot.busy !== "idle",
    supportLine: "Engine ready · Local processing",
    supportTone: "success",
  };
}
