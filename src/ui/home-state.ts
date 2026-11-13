import { t } from "../i18n";
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
      statusLabel: t("Checking"),
      title: t("Getting things ready"),
      description: t("Checking translation, recognition, and your subtitle area."),
      action: "none",
      actionLabel: t("Checking this PC…"),
      actionIcon: "spinner",
      actionDisabled: true,
      supportLine: t("Everything stays on this PC"),
      supportTone: "neutral",
    };
  }

  if (snapshot.running) {
    const warning = snapshot.captureWarning;
    return {
      state: "running",
      statusLabel: t("Running"),
      title: t("Subtitles are live"),
      description: t("Translated subtitles appear above your selected area."),
      action: "stop",
      actionLabel: snapshot.busy === "stopping" ? t("Stopping…") : t("Stop translation"),
      actionIcon: "stop",
      actionDisabled: snapshot.busy !== "idle",
      supportLine: warning
        ? describeCaptureWarning(warning)
        : t("Overlay active · Local processing"),
      supportTone: warning ? "warning" : "success",
    };
  }

  if (snapshot.busy === "selecting") {
    return {
      ...deriveHomePresentation({ ...snapshot, busy: "idle" }),
      statusLabel: t("Select area"),
      actionLabel: t("Confirm in the selector…"),
      actionDisabled: true,
    };
  }

  if (snapshot.busy === "warming" || snapshot.busy === "starting") {
    return {
      state: "checking",
      statusLabel: t("Starting"),
      title: t("Starting translation"),
      description: t("The first start takes a little longer while the engine warms up."),
      action: "none",
      actionLabel: t("Starting…"),
      actionIcon: "spinner",
      actionDisabled: true,
      supportLine: t("Preparing the engine on this PC"),
      supportTone: "neutral",
    };
  }

  const phase = snapshot.engine?.phase ?? "unknown";
  if (phase === backendUnavailablePhase) {
    return {
      state: "attention",
      statusLabel: t("Backend offline"),
      title: t("Start the browser backend"),
      description: t("Browser mode reads the engine through the local HTTP backend."),
      action: "none",
      actionLabel: t("Backend unavailable"),
      actionIcon: "alert",
      actionDisabled: true,
      supportLine: t("Run dev-browser.cmd to start the page with its backend"),
      supportTone: "danger",
    };
  }

  if (missingPhases.has(phase)) {
    return {
      state: "notReady",
      statusLabel: t("Setup needed"),
      title: t("Set up private translation"),
      description: t("A guided setup downloads and tests the translation engine."),
      action: "setup",
      actionLabel: t("Set up translation"),
      actionIcon: "download",
      actionDisabled: false,
      supportLine: t("Engine not installed · about 1.1 GB"),
      supportTone: "warning",
    };
  }

  if (preparingPhases.has(phase)) {
    return {
      state: "checking",
      statusLabel: phase === "busy" ? t("Busy") : t("Preparing"),
      title: phase === "busy" ? t("Waiting for the engine") : t("Warming up translation"),
      description:
        phase === "busy"
          ? t("The engine is finishing another operation. Check again when it is ready.")
          : t("The engine is checking its model. You can start when it is ready."),
      action: "none",
      actionLabel: phase === "busy" ? t("Waiting for engine…") : t("Preparing engine…"),
      actionIcon: "spinner",
      actionDisabled: true,
      supportLine:
        phase === "busy" ? t("Engine operation in progress") : t("Preparing the engine on this PC"),
      supportTone: "neutral",
    };
  }

  if (repairPhases.has(phase) || !readyPhases.has(phase)) {
    const supportCode = snapshot.engine?.supportCode;
    return {
      state: "attention",
      statusLabel: t("Needs repair"),
      title: t("Translation needs a repair"),
      description: t("Your settings are safe. Repair checks and restores the engine."),
      action: "repair",
      actionLabel: t("Repair engine"),
      actionIcon: "wrench",
      actionDisabled: false,
      supportLine: supportCode ? t("Support code") : t("Engine check failed · Repair available"),
      supportCode,
      supportTone: "danger",
    };
  }

  if (!recognitionReady(snapshot.settings, snapshot.ocrLanguages)) {
    return {
      state: "notReady",
      statusLabel: t("Almost ready"),
      title: t("Install the recognition language"),
      description: t("Windows needs this language to read the original subtitles."),
      action: "installOcr",
      actionLabel: t("Install recognition language"),
      actionIcon: "text",
      actionDisabled: snapshot.busy !== "idle",
      supportLine: t("Recognition language missing"),
      supportTone: "warning",
    };
  }

  return {
    state: "ready",
    statusLabel: t("Ready"),
    title: t("Let’s watch"),
    description: "",
    action: "start",
    actionLabel: t("Select subtitle area"),
    actionIcon: "area",
    actionDisabled: snapshot.busy !== "idle",
    supportLine: "",
    supportTone: "success",
  };
}
