import type { AppSettings } from "./contracts";

export type SettingsSaveState = { kind: "idle" | "saving" } | { kind: "error"; message: string };

/** Keep whole-settings writes ordered, including edits made while a save is pending. */
export class SettingsWriter {
  private tail: Promise<void> = Promise.resolve();
  private revision = 0;

  constructor(private readonly publish: (state: SettingsSaveState) => void) {}

  save(settings: AppSettings): Promise<void> {
    const revision = ++this.revision;
    const value = structuredClone(settings);
    this.publish({ kind: "saving" });
    const write = this.tail.then(async () => {
      try {
        await window.TauriBridge.invoke("save_settings", { settings: value });
        if (revision === this.revision) this.publish({ kind: "idle" });
      } catch (error) {
        if (revision === this.revision) {
          this.publish({
            kind: "error",
            message: error instanceof Error ? error.message : String(error),
          });
        }
        throw error;
      }
    });
    this.tail = write.catch(() => {});
    return write;
  }
}
