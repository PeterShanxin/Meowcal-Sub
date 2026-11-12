import type { EngineStatus } from "./contracts";

const COMPLETED_KEY = "meowcal.setupAcknowledged";

export class InitialSetup {
  private pending = false;

  async start(
    engine: EngineStatus | undefined,
    configuredModel: string | null,
  ): Promise<string | null> {
    const installed =
      Boolean(configuredModel) &&
      ["ready", "notRunning", "notrunning"].includes(engine?.phase ?? "");
    if (localStorage.getItem(COMPLETED_KEY) === "true") {
      await window.TauriBridge.invoke("show_main_after_setup");
      return null;
    }
    this.pending = true;
    try {
      if (installed) await window.TauriBridge.invoke("open_engine_wizard", { existing: true });
      else await window.TauriBridge.invoke("open_engine_wizard");
      return null;
    } catch (error) {
      this.pending = false;
      await window.TauriBridge.invoke("show_main_after_setup").catch(() => {});
      return error instanceof Error ? error.message : String(error);
    }
  }

  closed(payload: unknown): void {
    const completed = Boolean((payload as { modelDownloaded?: boolean } | null)?.modelDownloaded);
    if (completed) localStorage.setItem(COMPLETED_KEY, "true");
    if (this.pending) {
      this.pending = false;
      void window.TauriBridge.invoke(
        completed ? "show_main_after_setup" : "quit_initial_setup",
      ).catch(() => {});
    }
  }
}
