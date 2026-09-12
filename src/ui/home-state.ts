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
      description: "Checking translation, recognition, and your subtitle area.",
      action: "none",
      actionLabel: "Checking this PC…",
      actionIcon: "spinner",
      actionDisabled: true,
      supportLine: "Everything stays on this PC",
      supportTone: "neutral",
    };
  }

  if (snapshot.running) {
    return {
      state: "running",
      statusLabel: "Running",
      title: "Subtitles are live",
      description: "Translated subtitles appear above your selected area.",
      action: "stop",
      actionLabel: snapshot.busy === "stopping" ? "Stopping…" : "Stop translation",
      actionIcon: "stop",
      actionDisabled: snapshot.busy !== "idle",
      supportLine: "Overlay active · Local processing",
      supportTone: "success",
    };
  }

  if (snapshot.busy === "warming" || snapshot.busy === "starting") {
    return {
      state: "checking",
      statusLabel: "Starting",
      title: "Starting translation",
      description: "The first start takes a little longer while the engine warms up.",
      action: "none",
      actionLabel: "Starting…",
      actionIcon: "spinner",
      actionDisabled: true,
      supportLine: "Preparing the engine on this PC",
      supportTone: "neutral",
    };
  }

  const phase = snapshot.engine?.phase ?? "unknown";
  if (missingPhases.has(phase)) {
    return {
      state: "notReady",
      statusLabel: "Setup needed",
      title: "Set up private translation",
      description: "A guided setup downloads and tests the translation engine.",
      action: "setup",
      actionLabel: "Set up translation",
      actionIcon: "download",
      actionDisabled: false,
      supportLine: "Engine not installed · about 1.1 GB",
      supportTone: "warning",
    };
  }

  if (preparingPhases.has(phase)) {
    return {
      state: "checking",
      statusLabel: "Preparing",
      title: "Warming up translation",
      description: "The engine is checking its model. You can start when it is ready.",
      action: "none",
      actionLabel: "Preparing engine…",
      actionIcon: "spinner",
      actionDisabled: true,
      supportLine: "Preparing the engine on this PC",
      supportTone: "neutral",
    };
  }

  if (repairPhases.has(phase) || !readyPhases.has(phase)) {
    const supportCode = snapshot.engine?.supportCode;
    return {
      state: "attention",
      statusLabel: "Needs repair",
      title: "Translation needs a repair",
      description: "Your settings are safe. Repair checks and restores the engine.",
      action: "repair",
      actionLabel: "Repair engine",
      actionIcon: "wrench",
      actionDisabled: false,
      supportLine: supportCode ? "Support code" : "Engine check failed · Repair available",
      supportCode,
      supportTone: "danger",
    };
  }

  if (!ocrReady(snapshot)) {
    return {
      state: "notReady",
      statusLabel: "Almost ready",
      title: "Install the recognition language",
      description: "Windows needs this language to read the original subtitles.",
      action: "installOcr",
      actionLabel: "Install recognition language",
      actionIcon: "text",
      actionDisabled: snapshot.busy !== "idle",
      supportLine: "Recognition language missing",
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
      actionIcon: "area",
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
    actionIcon: "play",
    actionDisabled: snapshot.busy !== "idle",
    supportLine: "Engine ready · Local processing",
    supportTone: "success",
  };
}
