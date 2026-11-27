import type { PendingUpdate, UiSnapshot } from "./contracts";
import {
  advanceDownload,
  downloadPercent,
  NO_DOWNLOAD,
  shouldCheckForUpdatesAutomatically,
  UPDATE_CHECK_TIMEOUT_MS,
  type AutoUpdateCheckSettings,
  type UpdateCheckDiagnostic,
  type UpdateCheckErrorKind,
  type UpdateCheckIntent,
} from "./update-state";

export type { UpdateCheckIntent } from "./update-state";

class UpdateCheckTimeout extends Error {}
class UpdateCheckCancelled extends Error {}

const STORE_UPDATE = "store";
interface PortableUpdate {
  portableVersion: string;
}
type CheckedUpdate = PendingUpdate | typeof STORE_UPDATE | PortableUpdate;

interface ActiveCheck {
  requestId: number;
  source: UpdateCheckIntent;
  intent: UpdateCheckIntent;
  startedAtMs: number;
  startedAt: number;
  expired: boolean;
  cancelled: boolean;
  promise: Promise<void>;
  cancel(): void;
}

/**
 * Drives the update check and its apply step, and owns the one piece of state
 * that outlives a call: the update a check found, which the install then uses.
 *
 * Separate from `AppController` because the flow ends in the process exiting,
 * which is unlike every other action in the app and is worth reading on its own.
 */
export class UpdateController {
  private pending: CheckedUpdate | null = null;
  private storeManaged = false;
  private portable = false;
  private inFlightCheck: ActiveCheck | null = null;
  private nextCheckId = 0;
  private disposed = false;
  private applying = false;

  constructor(private readonly publish: (patch: Partial<UiSnapshot>) => void) {}

  private static message(error: unknown): string {
    return error instanceof Error ? error.message : String(error);
  }

  /** What the Settings screen should show before the user asks for anything. */
  async initialState(): Promise<Pick<UiSnapshot, "update" | "appVersion">> {
    const identifier = await window.TauriBridge.appIdentifier?.().catch(() => null);
    this.storeManaged =
      identifier === "com.meowcal.sub.store" || identifier === "com.meowcal.sub.store.dev";
    this.portable = identifier === "com.meowcal.sub.portable";
    if (this.storeManaged || this.portable) {
      const appVersion = (await window.TauriBridge.appVersion?.().catch(() => null)) ?? null;
      return { update: { kind: this.storeManaged ? "store" : "idle" }, appVersion };
    }
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
    const lookup: (() => Promise<CheckedUpdate | null>) | null = this.storeManaged
      ? async () =>
          (await window.TauriBridge.invoke<boolean>("check_store_update")) === true
            ? STORE_UPDATE
            : null
      : this.portable
        ? async () => {
            const portableVersion = await window.TauriBridge.invoke<string | null>(
              "check_portable_update",
            );
            return portableVersion ? { portableVersion } : null;
          }
        : updates
          ? () => updates.check({ timeout: UPDATE_CHECK_TIMEOUT_MS })
          : null;
    if (this.disposed || this.applying || !lookup) return;

    if (this.inFlightCheck) {
      if (intent === "manual" && this.inFlightCheck.intent === "automatic") {
        this.inFlightCheck.intent = "manual";
        this.replacePending(null);
        this.publish({ update: { kind: "checking" }, error: null, notice: null });
        this.record(this.inFlightCheck, "manualTakeover");
      }
      return this.inFlightCheck.promise;
    }

    const attempt: ActiveCheck = {
      requestId: ++this.nextCheckId,
      source: intent,
      intent,
      startedAtMs: Date.now(),
      startedAt: performance.now(),
      expired: false,
      cancelled: false,
      promise: Promise.resolve(),
      cancel: () => {},
    };
    if (intent === "manual") {
      this.replacePending(null);
      this.publish({ update: { kind: "checking" }, error: null, notice: null });
    }
    let timer!: ReturnType<typeof setTimeout>;
    const deadline = new Promise<CheckedUpdate | null>((_, reject) => {
      attempt.cancel = () => {
        attempt.cancelled = true;
        reject(new UpdateCheckCancelled());
      };
      timer = setTimeout(() => {
        attempt.expired = true;
        reject(new UpdateCheckTimeout());
      }, UPDATE_CHECK_TIMEOUT_MS);
    });
    this.inFlightCheck = attempt;
    this.record(attempt, "started");
    attempt.promise = this.runCheck(attempt, lookup, deadline, timer);
    return attempt.promise;
  }

  private async runCheck(
    attempt: ActiveCheck,
    lookup: () => Promise<CheckedUpdate | null>,
    deadline: Promise<CheckedUpdate | null>,
    timer: ReturnType<typeof setTimeout>,
  ): Promise<void> {
    const request = Promise.resolve()
      .then(lookup)
      .then((update) => {
        // The native IPC can finish after our deadline or after the page closes.
        if (attempt.expired || attempt.cancelled) {
          this.close(update);
          return null;
        }
        return update;
      });
    try {
      const update = await Promise.race([request, deadline]);
      if (attempt.cancelled) {
        this.close(update);
        throw new UpdateCheckCancelled();
      }
      this.replacePending(update);
      this.publish({
        update:
          update === STORE_UPDATE
            ? { kind: "storeAvailable" }
            : update && "portableVersion" in update
              ? { kind: "portableAvailable", version: update.portableVersion }
              : update
                ? { kind: "available", version: update.version, notes: update.notes }
                : { kind: "upToDate" },
      });
      this.record(attempt, "finished", { result: update ? "available" : "upToDate" });
    } catch (error) {
      if (attempt.cancelled || error instanceof UpdateCheckCancelled) {
        this.record(attempt, "finished", { result: "cancelled" });
        return;
      }
      const message = UpdateController.message(error);
      const errorKind: UpdateCheckErrorKind =
        error instanceof UpdateCheckTimeout || /timed?\s*out|timeout/i.test(message)
          ? "timeout"
          : /network|connect|dns|error sending request/i.test(message)
            ? "network"
            : "unknown";
      this.record(attempt, "finished", {
        result: errorKind === "timeout" ? "timeout" : "failed",
        errorKind,
      });
      if (attempt.intent === "manual") {
        this.publish({
          update: {
            kind: "error",
            message:
              errorKind === "timeout"
                ? "Cannot reach the update service right now. Try again."
                : message,
          },
        });
      } else {
        console.warn("[Meowcal] automatic update check failed", { errorKind });
      }
    } finally {
      clearTimeout(timer);
      if (this.inFlightCheck === attempt) this.inFlightCheck = null;
    }
  }

  private record(
    attempt: ActiveCheck,
    phase: UpdateCheckDiagnostic["phase"],
    outcome: Pick<UpdateCheckDiagnostic, "result" | "errorKind"> = {},
  ): void {
    const event: UpdateCheckDiagnostic = {
      requestId: attempt.requestId,
      source: attempt.source,
      intent: attempt.intent,
      phase,
      startedAtMs: attempt.startedAtMs,
      elapsedMs: Math.max(0, Math.round(performance.now() - attempt.startedAt)),
      ...outcome,
    };
    void window.TauriBridge.invoke("record_update_check", { event }).catch(() => {
      console.warn("[Meowcal] update diagnostics unavailable");
    });
  }

  private close(update: CheckedUpdate | null): void {
    if (update && update !== STORE_UPDATE && !("portableVersion" in update))
      void update.close().catch(() => console.warn("[Meowcal] update resource close failed"));
  }

  private replacePending(update: CheckedUpdate | null): void {
    if (this.pending !== update) this.close(this.pending);
    this.pending = update;
  }

  async saveAutomaticCheckTime(completedAtMs: number): Promise<void> {
    await window.TauriBridge.invoke("record_automatic_update_check", { completedAtMs }).catch(
      () => {},
    );
  }

  dispose(): void {
    this.disposed = true;
    this.inFlightCheck?.cancel();
    this.replacePending(null);
  }

  async checkAutomatic(
    settings: AutoUpdateCheckSettings,
    clock: () => number = () => Date.now(),
  ): Promise<number | null> {
    const updates = window.TauriBridge.updates;
    if (
      this.disposed ||
      this.applying ||
      window.TauriBridge.isBrowserMode() ||
      (!this.storeManaged && !this.portable && !updates)
    )
      return null;
    if (!shouldCheckForUpdatesAutomatically(settings, clock())) return null;
    const checking = this.check("automatic");
    const attempt = this.inFlightCheck;
    await checking;
    return this.disposed || attempt?.cancelled ? null : clock();
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
    const checking = this.inFlightCheck;
    if (this.storeManaged) {
      if (this.disposed || this.applying || update !== STORE_UPDATE) return;
      this.applying = true;
      try {
        await window.TauriBridge.invoke("open_store_listing");
      } catch {
        this.replacePending(null);
        if (!this.disposed)
          this.publish({
            update: {
              kind: "error",
              message: "Could not open Microsoft Store. Try opening it from Start.",
            },
          });
      } finally {
        this.applying = false;
      }
      return;
    }
    if (this.portable) {
      if (
        this.disposed ||
        this.applying ||
        !update ||
        update === STORE_UPDATE ||
        !("portableVersion" in update)
      )
        return;
      this.applying = true;
      try {
        await window.TauriBridge.invoke("open_portable_update", {
          version: update.portableVersion,
        });
      } catch (error) {
        if (!this.disposed)
          this.publish({ update: { kind: "error", message: UpdateController.message(error) } });
      } finally {
        this.applying = false;
      }
      return;
    }
    if (
      this.disposed ||
      this.applying ||
      (checking && checking.intent !== "automatic") ||
      !updates ||
      !update ||
      update === STORE_UPDATE ||
      "portableVersion" in update
    )
      return;

    this.applying = true;
    // The installation owns this resource until its handoff finishes.
    this.pending = null;
    this.publish({ update: { kind: "downloading", version: update.version, percent: null } });
    try {
      if (checking) {
        checking.cancel();
        await checking.promise;
      }
      if (this.disposed) return;
      await window.TauriBridge.invoke("prepare_for_update");
      if (this.disposed) return;
      let progress = NO_DOWNLOAD;
      await update.install((event) => {
        if (this.disposed) return;
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
      if (!this.disposed)
        this.publish({ running: false, update: { kind: "installing", version: update.version } });
      await updates.restart();
    } catch (error) {
      if (!this.disposed)
        this.publish({ update: { kind: "error", message: UpdateController.message(error) } });
    } finally {
      this.close(update);
      this.applying = false;
    }
  }
}
