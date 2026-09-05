import type { PendingUpdate, UiSnapshot } from "./contracts";
import {
  advanceDownload,
  downloadPercent,
  NO_DOWNLOAD,
  shouldCheckForUpdatesAutomatically,
  type AutoUpdateCheckSettings,
} from "./update-state";

export type UpdateCheckIntent = "manual" | "automatic";

/**
 * Drives the update check and its apply step, and owns the one piece of state
 * that outlives a call: the update a check found, which the install then uses.
 *
 * Separate from `AppController` because the flow ends in the process exiting,
 * which is unlike every other action in the app and is worth reading on its own.
 */
export class UpdateController {
  private pending: PendingUpdate | null = null;
  private checking = false;

  constructor(private readonly publish: (patch: Partial<UiSnapshot>) => void) {}

  private static message(error: unknown): string {
    return error instanceof Error ? error.message : String(error);
  }

  /** What the Settings screen should show before the user asks for anything. */
  async initialState(): Promise<Pick<UiSnapshot, "update" | "appVersion">> {
    const updates = window.TauriBridge.updates;
    if (!updates) return { update: { kind: "unsupported" }, appVersion: null };
    try {
      return { update: { kind: "idle" }, appVersion: await updates.currentVersion() };
    } catch (error) {
      console.warn("[Meowcal] app version unavailable", error);
      return { update: { kind: "idle" }, appVersion: null };
    }
  }

  async check(intent: UpdateCheckIntent = "manual"): Promise<void> {
    const updates = window.TauriBridge.updates;
    if (!updates || this.checking) return;
    this.checking = true;

    if (intent === "manual") {
      // Dropped before a manual check so a failed one cannot leave the previous
      // answer installable.
      this.pending = null;
      this.publish({ update: { kind: "checking" }, error: null, notice: null });
    }

    try {
      const update = await updates.check();
      if (!update) {
        this.pending = null;
        this.publish({ update: { kind: "upToDate" } });
        return;
      }
      this.pending = update;
      this.publish({
        update: { kind: "available", version: update.version, notes: update.notes },
      });
    } catch (error) {
      if (intent === "manual") {
        this.publish({ update: { kind: "error", message: UpdateController.message(error) } });
      } else {
        console.warn("[Meowcal] automatic update check failed", error);
      }
    } finally {
      this.checking = false;
    }
  }

  async checkAutomatic(
    settings: AutoUpdateCheckSettings,
    clock: () => number = () => Date.now(),
  ): Promise<number | null> {
    const updates = window.TauriBridge.updates;
    if (window.TauriBridge.isBrowserMode() || !updates) return null;
    if (!shouldCheckForUpdatesAutomatically(settings, clock())) return null;
    await this.check("automatic");
    return clock();
  }

  /**
   * Apply the update the last check found.
   *
   * The backend is quiesced first: the installer replaces files this app still
   * holds open, and on Windows it also ends this process, so anything that has
   * to happen before the handoff cannot wait for a shutdown event.
   *
   * `install` normally does not return - the process ends inside it. The
   * restart after it is for the case where it does.
   */
  async install(): Promise<void> {
    const updates = window.TauriBridge.updates;
    const update = this.pending;
    if (!updates || !update) return;

    this.publish({ update: { kind: "downloading", version: update.version, percent: null } });
    try {
      await window.TauriBridge.invoke("prepare_for_update");
      let progress = NO_DOWNLOAD;
      await update.install((event) => {
        progress = advanceDownload(progress, event);
        this.publish({
          update:
            event.event === "Finished"
              ? { kind: "installing", version: update.version }
              : {
                  kind: "downloading",
                  version: update.version,
                  percent: downloadPercent(progress),
                },
        });
      });
      // Capture stopped during the handoff, so the rest of the UI must not go
      // on claiming a session is live while the app is being replaced.
      this.publish({ running: false, update: { kind: "installing", version: update.version } });
      await updates.restart();
    } catch (error) {
      this.pending = null;
      this.publish({ update: { kind: "error", message: UpdateController.message(error) } });
    }
  }
}
