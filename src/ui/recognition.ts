import type { AppSettings, OcrEngine } from "./contracts";

/**
 * The recognizer a session will use. PP-OCRv6 has no Hangul, so Korean reads
 * with Windows OCR whatever the setting says; `ocr::session_engine` in Rust
 * applies the same rule.
 */
export function sessionEngine(settings: AppSettings): OcrEngine {
  return settings.sourceLanguage.toLowerCase().startsWith("ko")
    ? "windows"
    : settings.translation.ocr.engine;
}

/** Only Windows OCR needs a Windows language pack for the original subtitles. */
export function recognitionReady(settings: AppSettings, installed: ReadonlySet<string>): boolean {
  return (
    sessionEngine(settings) === "ppOcrV6" ||
    window.OcrLanguageTags.isOcrLanguageAvailable(installed, settings.sourceLanguage)
  );
}
