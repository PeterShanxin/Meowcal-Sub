import { describeError } from "./error-copy";
import { t } from "../i18n";
/**
 * What the Settings screen shows about updates, and how a download's progress
 * events turn into a number.
 *
 * Kept apart from the plugin call itself so both are testable: the reducer here
 * never touches `window`, and `tauri-bridge` never decides wording.
 */

export type UpdateStatus =
  | { kind: "store" }
  | { kind: "unsupported" }
  | { kind: "idle" }
  | { kind: "checking" }
  | { kind: "upToDate" }
  | { kind: "available"; version: string; notes: string | null }
  | { kind: "downloading"; version: string; percent: number | null }
  | { kind: "installing"; version: string }
  | { kind: "error"; message: string };

export type UpdateAction = "check" | "install" | "none";

export interface UpdatePresentation {
  headline: string;
  detail: string;
  action: UpdateAction;
  actionLabel: string;
  actionDisabled: boolean;
  /** Release notes, only when there is something worth reading before applying. */
  notes: string | null;
}

/** Bytes seen so far against the total the server declared, if it declared one. */
export interface DownloadProgress {
  received: number;
  total: number | null;
}

/**
 * A progress event from `downloadAndInstall`.
 *
 * `Started` carries the total, `Progress` carries one chunk's length - never a
 * running total - and `Finished` carries nothing. Treating `chunkLength` as a
 * position is the mistake this shape invites.
 */
export type DownloadEvent =
  | { event: "Started"; data?: { contentLength?: number | null } }
  | { event: "Progress"; data?: { chunkLength?: number | null } }
  | { event: "Finished" };

export const NO_DOWNLOAD: DownloadProgress = { received: 0, total: null };

export function advanceDownload(
  progress: DownloadProgress,
  event: DownloadEvent,
): DownloadProgress {
  switch (event.event) {
    case "Started": {
      const total = event.data?.contentLength ?? null;
      return { received: 0, total: total && total > 0 ? total : null };
    }
    case "Progress":
      return {
        ...progress,
        received: progress.received + (event.data?.chunkLength ?? 0),
      };
    case "Finished":
      return { ...progress, received: progress.total ?? progress.received };
  }
}

/**
 * Whole percent, or `null` when the server never said how large the download
 * is. A bar that cannot be honest about its position should not draw one.
 */
export function downloadPercent(progress: DownloadProgress): number | null {
  if (progress.total === null || progress.total <= 0) return null;
  const ratio = progress.received / progress.total;
  return Math.min(100, Math.max(0, Math.round(ratio * 100)));
}

function installedLine(currentVersion: string | null): string {
  return currentVersion
    ? t("Installed version {version}", { version: currentVersion })
    : t("Installed version unknown");
}

export function deriveUpdatePresentation(
  status: UpdateStatus,
  currentVersion: string | null,
): UpdatePresentation {
  switch (status.kind) {
    case "store":
      return {
        headline: t("Microsoft Store updates"),
        detail: t("{installed}. Get updates from the Microsoft Store Library.", {
          installed: installedLine(currentVersion),
        }),
        action: "none",
        actionLabel: t("Managed by Microsoft Store"),
        actionDisabled: true,
        notes: null,
      };
    case "unsupported":
      return {
        headline: t("Updates"),
        detail: t("Updating is handled by the installed app, not the browser preview."),
        action: "none",
        actionLabel: t("Unavailable here"),
        actionDisabled: true,
        notes: null,
      };
    case "checking":
      return {
        headline: t("Checking for updates…"),
        detail: installedLine(currentVersion),
        action: "none",
        actionLabel: t("Checking…"),
        actionDisabled: true,
        notes: null,
      };
    case "upToDate":
      return {
        headline: t("Meowcal Sub is up to date"),
        detail: installedLine(currentVersion),
        action: "check",
        actionLabel: t("Check again"),
        actionDisabled: false,
        notes: null,
      };
    case "available":
      return {
        headline: t("Version {version} is available", { version: status.version }),
        detail: t(
          "Installing closes Meowcal Sub, replaces it, and reopens it. Your settings and engine stay.",
        ),
        action: "install",
        actionLabel: t("Download and install"),
        actionDisabled: false,
        notes: status.notes,
      };
    case "downloading":
      return {
        headline: t("Downloading {version}…", { version: status.version }),
        detail:
          status.percent === null
            ? t("Download size unknown; this may take a moment.")
            : t("{percent}% downloaded", { percent: status.percent }),
        action: "none",
        actionLabel: t("Downloading…"),
        actionDisabled: true,
        notes: null,
      };
    case "installing":
      return {
        headline: t("Installing {version}…", { version: status.version }),
        detail: t("Meowcal Sub will close and reopen on its own."),
        action: "none",
        actionLabel: t("Installing…"),
        actionDisabled: true,
        notes: null,
      };
    case "error":
      return {
        headline: t("Update check failed"),
        detail: describeError(status.message).text,
        action: "check",
        actionLabel: t("Try again"),
        actionDisabled: false,
        notes: null,
      };
    case "idle":
      return {
        headline: t("Updates"),
        detail: installedLine(currentVersion),
        action: "check",
        actionLabel: t("Check for updates"),
        actionDisabled: false,
        notes: null,
      };
  }
}

export const UPDATE_CHECK_INTERVAL_MS = 24 * 60 * 60 * 1000;
export const UPDATE_CHECK_TIMEOUT_MS = 10_000;

export type UpdateCheckIntent = "manual" | "automatic";
export type UpdateCheckErrorKind = "timeout" | "network" | "unknown";
export interface UpdateCheckDiagnostic {
  requestId: number;
  source: UpdateCheckIntent;
  intent: UpdateCheckIntent;
  phase: "started" | "manualTakeover" | "finished";
  startedAtMs: number;
  elapsedMs: number;
  result?: "upToDate" | "available" | "failed" | "timeout" | "cancelled";
  errorKind?: UpdateCheckErrorKind;
}

export interface AutoUpdateCheckSettings {
  autoCheckUpdates?: boolean;
  lastUpdateCheckTimeMs?: number | null;
}

export function shouldCheckForUpdatesAutomatically(
  settings: AutoUpdateCheckSettings,
  nowMs: number,
  intervalMs: number = UPDATE_CHECK_INTERVAL_MS,
): boolean {
  if (settings.autoCheckUpdates === false) {
    return false;
  }
  const last = settings.lastUpdateCheckTimeMs;
  if (typeof last !== "number" || !Number.isFinite(last)) {
    return true;
  }
  const elapsed = nowMs - last;
  return elapsed >= intervalMs || elapsed < 0;
}
