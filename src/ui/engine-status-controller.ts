import type { EngineStatus, UiSnapshot } from "./contracts";

const RECHECK_INTERVAL_MS = 1000;
const MAX_RECHECKS = 120;

export class EngineStatusController {
  private revision = 0;
  private disposed = false;
  private current: EngineStatus | undefined;
  private preparation: Promise<EngineStatus> | null = null;
  private timer: number | null = null;
  private checking = false;
  private attempts = 0;

  constructor(private readonly publish: (patch: Partial<UiSnapshot>) => void) {}

  async read(command: string, fallback: EngineStatus): Promise<EngineStatus> {
    const revision = ++this.revision;
    let engine: EngineStatus;
    try {
      engine = await window.TauriBridge.invoke<EngineStatus>(command);
    } catch (error) {
      console.warn(`[Meowcal] ${command} unavailable`, error);
      engine = fallback;
    }
    if (!this.disposed && revision === this.revision) this.accept(engine);
    return engine;
  }

  private accept(engine: EngineStatus): void {
    if (this.current?.phase !== "busy") this.attempts = 0;
    this.current = engine;
    this.publish({ engine });
    if (engine?.phase === "busy") this.scheduleRecheck();
    else this.cancelTimer();
  }

  private scheduleRecheck(): void {
    if (this.disposed || this.timer !== null || this.checking || this.attempts >= MAX_RECHECKS)
      return;
    this.timer = window.setTimeout(() => {
      this.timer = null;
      void this.recheck();
    }, RECHECK_INTERVAL_MS);
  }

  private async recheck(): Promise<void> {
    const revision = this.revision;
    this.checking = true;
    this.attempts++;
    try {
      const engine = await window.TauriBridge.invoke<EngineStatus>("refresh_engine_status");
      if (!this.disposed && revision === this.revision) this.accept(engine);
    } catch (error) {
      if (!this.disposed && revision === this.revision) {
        this.accept({ ...this.current, phase: "error" });
        this.publish({ error: error instanceof Error ? error.message : String(error) });
      }
    } finally {
      this.checking = false;
      if (this.current?.phase === "busy") this.scheduleRecheck();
    }
  }

  async finishPreparation(engine: EngineStatus | undefined): Promise<void> {
    if (this.disposed || engine !== this.current || engine?.phase !== "preparing") return;
    await this.completePreparation(engine);
  }

  async prewarm(): Promise<void> {
    const engine = this.current;
    if (this.disposed || !engine || !["notRunning", "notrunning"].includes(engine.phase ?? ""))
      return;
    await this.completePreparation(engine);
  }

  private async completePreparation(engine: EngineStatus): Promise<void> {
    const revision = this.revision;
    try {
      const ready = await this.prepare();
      if (!this.disposed && revision === this.revision) this.accept(ready);
    } catch (error) {
      if (!this.disposed && revision === this.revision) {
        this.accept({ ...engine, phase: "error" });
        this.publish({ error: error instanceof Error ? error.message : String(error) });
      }
    }
  }

  private prepare(): Promise<EngineStatus> {
    this.preparation ??= window.TauriBridge.invoke<EngineStatus>("make_engine_ready").finally(
      () => {
        this.preparation = null;
      },
    );
    return this.preparation;
  }

  async ready(): Promise<EngineStatus> {
    const revision = ++this.revision;
    // A status read during preparation can report busy; join it before checking again.
    if (this.preparation) await this.preparation;
    let engine = await window.TauriBridge.invoke<EngineStatus>("refresh_engine_status");
    if (["notRunning", "notrunning", "preparing"].includes(engine.phase ?? "")) {
      engine = await this.prepare();
    }
    if (this.disposed || revision !== this.revision)
      throw new Error("Engine status changed. Try again.");
    this.accept(engine);
    if (engine.phase !== "ready") throw new Error("The local translation engine is not ready yet.");
    return engine;
  }

  private cancelTimer(): void {
    if (this.timer !== null) window.clearTimeout(this.timer);
    this.timer = null;
  }

  dispose(): void {
    this.disposed = true;
    this.cancelTimer();
  }
}
