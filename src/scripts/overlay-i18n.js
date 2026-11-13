import { t, subscribeLocale } from "../i18n/index";
import { initializeLocale } from "../i18n/runtime";

/** Text bindings leave subtitles, captured text, focus and selection geometry intact. */
export function localizeOverlayDocument() {
  for (const element of document.querySelectorAll("[data-i18n]")) {
    element.textContent = t(element.dataset.i18n);
  }
  for (const attribute of ["title", "aria-label"]) {
    for (const element of document.querySelectorAll(`[data-i18n-${attribute}]`)) {
      element.setAttribute(attribute, t(element.getAttribute(`data-i18n-${attribute}`)));
    }
  }
}

export function initializeOverlayLocale() {
  localizeOverlayDocument();
  const unsubscribe = subscribeLocale(localizeOverlayDocument);
  const initialized = initializeLocale();
  void initialized.then(localizeOverlayDocument);
  window.addEventListener(
    "pagehide",
    () => {
      unsubscribe();
      void initialized.then((dispose) => dispose());
    },
    { once: true },
  );
}
