import type { AppSettings } from "./contracts";
import { confirmUiLanguage, type UiLanguage } from "../i18n";

export type SettingsSaveState = { kind: "idle" | "saving" } | { kind: "error"; message: string };
type Pending<T> = { value: T; saved: boolean; error?: string };

/** Order explicit settings and language edits without turning a locale edit into a full save. */
export class SettingsWriter {
  private tail: Promise<void> = Promise.resolve();
  private revision = 0;
  private deferredSettings = false;
  private settings?: Pending<AppSettings>;
  private language?: Pending<UiLanguage>;

  constructor(private readonly publish: (state: SettingsSaveState) => void) {}

  markPending(): void {
    this.deferredSettings = true;
    ++this.revision;
    this.publish({ kind: "saving" });
  }

  save(settings: AppSettings): Promise<void> {
    this.deferredSettings = false;
    this.settings = { value: structuredClone(settings), saved: false };
    return this.enqueue(this.settings);
  }

  saveLanguage(language: UiLanguage): Promise<void> {
    this.language = { value: language, saved: false };
    return this.enqueue(undefined, this.language);
  }

  retry(): Promise<void> {
    return this.enqueue(this.settings, this.language);
  }

  private enqueue(settings?: Pending<AppSettings>, language?: Pending<UiLanguage>): Promise<void> {
    const revision = ++this.revision;
    this.publish({ kind: "saving" });
    const write = this.tail.then(async () => {
      let active: Pending<unknown> | undefined;
      try {
        if (settings && !settings.saved) {
          active = settings;
          await window.TauriBridge.invoke("save_settings", { settings: settings.value });
          settings.saved = true;
        }
        if (language && !language.saved) {
          active = language;
          await window.TauriBridge.invoke("set_ui_language", { language: language.value });
          language.saved = true;
          if (language === this.language) confirmUiLanguage(language.value);
        }
      } catch (error) {
        if (active) active.error = error instanceof Error ? error.message : String(error);
        throw error;
      } finally {
        if (revision === this.revision) this.publishPendingState();
      }
    });
    this.tail = write.catch(() => {});
    return write;
  }

  private publishPendingState(): void {
    const pending = [this.settings, this.language].filter((edit) => edit && !edit.saved);
    const failed = pending.find((edit) => edit?.error !== undefined);
    if (failed?.error !== undefined) this.publish({ kind: "error", message: failed.error });
    else this.publish({ kind: this.deferredSettings || pending.length ? "saving" : "idle" });
  }
}
