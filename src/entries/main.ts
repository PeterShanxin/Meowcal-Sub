import "../scripts/tauri-bridge.js";
import "../scripts/ocr-language-tags.js";
import "../scripts/overlay-appearance.js";
import "../ui/meowcal-app";

import { initializeLocale } from "../i18n/runtime";
void initializeLocale().then((dispose) =>
  window.addEventListener("pagehide", dispose, { once: true }),
);
