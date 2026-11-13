import type { ReactiveController, ReactiveControllerHost } from "lit";
import { subscribeLocale } from "./index";

export class LocaleController implements ReactiveController {
  private unsubscribe?: () => void;
  constructor(private readonly host: ReactiveControllerHost) {
    host.addController(this);
  }
  hostConnected(): void {
    this.unsubscribe = subscribeLocale(() => this.host.requestUpdate());
  }
  hostDisconnected(): void {
    this.unsubscribe?.();
  }
}
