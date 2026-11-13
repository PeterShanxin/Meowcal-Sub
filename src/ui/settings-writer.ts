import type { AppSettings } from "./contracts";
import { confirmUiLanguage, type UiLanguage } from "../i18n";

export type SettingsSaveState = { kind: "idle" | "saving" } | { kind: "error"; message: string };
type Pending<T> = { value: T; saved: boolean };

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
    return this.retry();
  }

  saveLanguage(language: UiLanguage): Promise<void> {
    this.language = { value: language, saved: false };
    return this.retry();
  }

  retry(): Promise<void> {
    const revision = ++this.revision;
    const settings = this.settings;
    const language = this.language;
    this.publish({ kind: "saving" });
    const write = this.tail.then(async () => {
      try {
        if (settings && !settings.saved) {
          await window.TauriBridge.invoke("save_settings", { settings: settings.value });
          settings.saved = true;
        }
        if (language && !language.saved) {
          await window.TauriBridge.invoke("set_ui_language", { language: language.value });
          language.saved = true;
          if (language === this.language) confirmUiLanguage(language.value);
        }
        if (revision === this.revision && !this.deferredSettings) this.publish({ kind: "idle" });
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
