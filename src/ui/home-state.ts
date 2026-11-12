import type { HomePresentation, UiSnapshot } from "./contracts";
import { describeCaptureWarning } from "./error-copy";
import { recognitionReady } from "./recognition";

const repairPhases = new Set(["error", "noModels", "nomodels", "damaged", "invalid"]);
const missingPhases = new Set(["notInstalled", "notinstalled"]);
const preparingPhases = new Set(["preparing", "busy"]);
const readyPhases = new Set(["ready", "notRunning", "notrunning"]);
/** Browser mode only: its HTTP backend did not answer. Core never reports it. */
export const backendUnavailablePhase = "backendUnavailable";

/**
 * The language pair and subtitle area feed the session, so they cannot change
 * while one is starting, running, or stopping.
 */
export function isSessionLocked(snapshot: UiSnapshot): boolean {
  return (
    snapshot.running ||
    snapshot.busy === "selecting" ||
    snapshot.busy === "warming" ||
    snapshot.busy === "starting" ||
    snapshot.busy === "stopping"
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
    const warning = snapshot.captureWarning;
    return {
      state: "running",
      statusLabel: "Running",
      title: "Subtitles are live",
      description: "Translated subtitles appear above your selected area.",
      action: "stop",
      actionLabel: snapshot.busy === "stopping" ? "Stopping…" : "Stop translation",
      actionIcon: "stop",
      actionDisabled: snapshot.busy !== "idle",
      supportLine: warning ? describeCaptureWarning(warning) : "Overlay active · Local processing",
      supportTone: warning ? "warning" : "success",
    };
  }

  if (snapshot.busy === "selecting") {
    return {
      state: "checking",
      statusLabel: "Select area",
      title: "Confirm the subtitle area",
      description: "Confirm the saved box or adjust it to fit the original subtitles.",
      action: "none",
      actionLabel: "Confirm in the selector…",
      actionIcon: "area",
      actionDisabled: true,
      supportLine: "Cancel to return without starting",
      supportTone: "neutral",
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
  if (phase === backendUnavailablePhase) {
    return {
      state: "attention",
      statusLabel: "Backend offline",
      title: "Start the browser backend",
      description: "Browser mode reads the engine through the local HTTP backend.",
      action: "none",
      actionLabel: "Backend unavailable",
      actionIcon: "alert",
      actionDisabled: true,
      supportLine: "Run dev-browser.cmd to start the page with its backend",
      supportTone: "danger",
    };
  }

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
      statusLabel: phase === "busy" ? "Busy" : "Preparing",
      title: phase === "busy" ? "Waiting for the engine" : "Warming up translation",
      description:
        phase === "busy"
          ? "The engine is finishing another operation. Check again when it is ready."
          : "The engine is checking its model. You can start when it is ready.",
      action: "none",
      actionLabel: phase === "busy" ? "Waiting for engine…" : "Preparing engine…",
      actionIcon: "spinner",
      actionDisabled: true,
      supportLine:
        phase === "busy" ? "Engine operation in progress" : "Preparing the engine on this PC",
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

  if (!recognitionReady(snapshot.settings, snapshot.ocrLanguages)) {
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
      description: "Start translation, then confirm a box around the original subtitles.",
      action: "start",
      actionLabel: "Start translation",
      actionIcon: "area",
      actionDisabled: false,
      supportLine: "",
      supportTone: "warning",
    };
  }

  return {
    state: "ready",
    statusLabel: "Ready",
    title: "Let’s watch",
    description: "",
    action: "start",
    actionLabel: "Start translation",
    actionIcon: "play",
    actionDisabled: snapshot.busy !== "idle",
    supportLine: "",
    supportTone: "success",
  };
}
