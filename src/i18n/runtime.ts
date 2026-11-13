import {
  acceptSavedUiLanguage,
  getUiLanguage,
  getPreferenceRevision,
  setUiLanguage,
  subscribeLocale,
  t,
} from "./index";

/** One preference owner per webview; the native config is the durable source. */
export async function initializeLocale(): Promise<() => void> {
  const bridge = window.TauriBridge;
  const systemLanguages = async (): Promise<readonly string[]> =>
    bridge.isBrowserMode()
      ? navigator.languages
      : [await bridge.invoke<string>("get_system_ui_language")];
  let languages = await systemLanguages().catch(() => navigator.languages);
  setUiLanguage(getUiLanguage(), languages);
  let revision = 0;
  let systemRevision = 0;
  let disposed = false;
  const unlisten = await bridge.event.listen("ui-language-changed", ({ payload }) => {
    if (disposed) return;
    ++revision;
    acceptSavedUiLanguage(payload);
  });
  const initialRevision = revision;
  const initialPreferenceRevision = getPreferenceRevision();
  try {
    const settings = await bridge.invoke<{ uiLanguage?: unknown }>("get_settings");
    if (revision === initialRevision)
      acceptSavedUiLanguage(settings.uiLanguage, initialPreferenceRevision);
  } catch {
    // Offline browser previews still use the browser's preferred language.
    setUiLanguage(getUiLanguage(), languages);
  }
  const systemChanged = async () => {
    const request = ++systemRevision;
    const next = await systemLanguages().catch(() => languages);
    if (disposed || request !== systemRevision) return;
    languages = next;
    setUiLanguage(getUiLanguage(), languages);
  };
  window.addEventListener("languagechange", systemChanged);
  const localizePreview = () => {
    const indicator =
      typeof document === "undefined" ? null : document.getElementById?.("browser-mode-indicator");
    if (!indicator) return;
    indicator.textContent = t("Browser preview");
    indicator.title = t("Capture, translation, and native windows require the Windows app.");
  };
  localizePreview();
  const stopPreview = subscribeLocale(localizePreview);
  // The classic bridge creates its badge on DOMContentLoaded when loaded early.
  window.addEventListener("DOMContentLoaded", localizePreview, { once: true });
  return () => {
    disposed = true;
    unlisten();
    stopPreview();
    window.removeEventListener("DOMContentLoaded", localizePreview);
    window.removeEventListener("languagechange", systemChanged);
  };
}
