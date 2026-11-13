import "../scripts/tauri-bridge.js";
import "../scripts/ocr-language-tags.js";
import "../ui/meowcal-setup";

import { initializeLocale } from "../i18n/runtime";
void initializeLocale().then((dispose) =>
  window.addEventListener("pagehide", dispose, { once: true }),
);
