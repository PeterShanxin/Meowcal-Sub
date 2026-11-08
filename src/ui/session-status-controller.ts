import type { UiSnapshot } from "./contracts";

type Publish = (patch: Partial<UiSnapshot>) => void;
type Position = { sessionId: number; captureId: number };

/** Keeps Home errors current without publishing ordinary subtitle activity. */
export class SessionStatusController {
  private latest: Position = { sessionId: 0, captureId: -1 };
  private retiredSession = 0;
  private captureError: string | null = null;

  constructor(
    private readonly publish: Publish,
    private readonly current: () => UiSnapshot,
  ) {}

  reset(): void {
    this.retiredSession = Math.max(this.retiredSession, this.latest.sessionId);
    this.captureError = null;
  }

  async listen(): Promise<Array<() => void>> {
    return Promise.all([
      window.TauriBridge.event.listen("subtitle-activity", ({ payload }) => {
        this.activity(payload);
      }),
      window.TauriBridge.event.listen("capture-status", ({ payload }) => {
        const status = payload as { isError?: boolean; message?: string } | null;
        if (status?.isError) {
          this.captureError = status.message ?? "Screen capture failed";
          this.publish({ error: this.captureError });
        } else if (status?.message) {
          this.publish({ captureWarning: status.message });
        }
      }),
      window.TauriBridge.event.listen("translation-interrupted", ({ payload }) => {
        const sessionId = (payload as { sessionId?: unknown } | null)?.sessionId;
        if (
          typeof sessionId !== "number" ||
          !Number.isSafeInteger(sessionId) ||
          sessionId <= this.retiredSession ||
          sessionId < this.latest.sessionId
        ) {
          return;
        }
        this.retiredSession = sessionId;
        this.captureError ??= "Translation stopped unexpectedly. Start translation again.";
        this.publish({
          running: false,
          busy: "idle",
          notice: null,
          error: this.captureError,
        });
      }),
      window.TauriBridge.event.listen("translation-exited", () => {
        const snapshot = this.current();
        const error =
          !snapshot.running && snapshot.error === this.captureError ? this.captureError : null;
        this.reset();
        this.publish({
          screen: "home",
          running: false,
          busy: "idle",
          error,
          captureWarning: null,
          notice: error ? null : "Translation stopped",
        });
      }),
    ]);
  }

  private activity(payload: unknown): void {
    if (!payload || typeof payload !== "object") return;
    const next = payload as Position & { state?: string };
    if (
      !Number.isSafeInteger(next.sessionId) ||
      !Number.isSafeInteger(next.captureId) ||
      next.captureId < 0 ||
      next.sessionId <= this.retiredSession ||
      (next.state !== "active" && next.state !== "waiting")
    ) {
      return;
    }
    const snapshot = this.current();
    if (!snapshot.running && snapshot.busy !== "starting") return;
    if (
      next.sessionId < this.latest.sessionId ||
      (next.sessionId === this.latest.sessionId && next.captureId <= this.latest.captureId)
    ) {
      return;
    }
    this.latest = { sessionId: next.sessionId, captureId: next.captureId };
    if (this.captureError !== null) {
      if (snapshot.error === this.captureError) this.publish({ error: null });
      this.captureError = null;
    }
  }
}
