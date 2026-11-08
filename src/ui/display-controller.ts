import type { CaptureRegion, UiSnapshot } from "./contracts";

type DisplayContext = { multipleDisplays: boolean; region: CaptureRegion | null };

export class DisplayController {
  private timer: number | null = null;
  private unlisten: Array<() => void> = [];
  private disposed = false;
  private revision = 0;

  constructor(private readonly publish: (patch: Partial<UiSnapshot>) => void) {}

  async read(): Promise<Partial<UiSnapshot>> {
    if (window.TauriBridge.isBrowserMode()) return {};
    try {
      const context = await window.TauriBridge.invoke<DisplayContext>("get_display_context");
      return context ? { multipleDisplays: context.multipleDisplays, region: context.region } : {};
    } catch (error) {
      console.warn("[Meowcal] Display context unavailable", error);
      return {};
    }
  }

  async start(): Promise<void> {
    this.unlisten.push(
      await window.TauriBridge.event.listen("display-context-changed", () => {
        const revision = ++this.revision;
        if (this.timer !== null) window.clearTimeout(this.timer);
        this.timer = window.setTimeout(async () => {
          this.timer = null;
          const patch = await this.read();
          if (!this.disposed && revision === this.revision) this.publish(patch);
        }, 150);
      }),
      await window.TauriBridge.event.listen("display-invalidated", (event) => {
        ++this.revision;
        if (this.timer !== null) window.clearTimeout(this.timer);
        this.timer = null;
        this.publish({
          running: false,
          busy: "idle",
          region: null,
          notice: null,
          captureWarning: null,
          error: String(event.payload),
        });
      }),
    );
  }

  dispose(): void {
    this.disposed = true;
    if (this.timer !== null) window.clearTimeout(this.timer);
    this.unlisten.splice(0).forEach((unlisten) => unlisten());
  }
}
