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

/** The choices that decide how much CPU a session takes. */
export interface RecognitionChoice {
  engine: OcrEngine;
  cpuOnly: boolean;
}

export type Profile = "automatic" | "compatibility";

/**
 * Automatic lets translation use the graphics card when Core can, and Core
 * then gives PP-OCRv6 its larger thread share. Compatibility avoids both the
 * graphics driver and PP-OCRv6's CPU load.
 */
export const profiles: Record<Profile, RecognitionChoice> = {
  automatic: { engine: "ppOcrV6", cpuOnly: false },
  compatibility: { engine: "windows", cpuOnly: true },
};

export function profileOf(settings: AppSettings): Profile | "custom" {
  const engine = settings.translation.ocr.engine;
  const cpuOnly = settings.translation.localEngine.cpuOnly;
  const match = (Object.keys(profiles) as Profile[]).find(
    (name) => profiles[name].engine === engine && profiles[name].cpuOnly === cpuOnly,
  );
  return match ?? "custom";
}

export function applyRecognition(settings: AppSettings, choice: Partial<RecognitionChoice>): void {
  if (choice.engine !== undefined) settings.translation.ocr.engine = choice.engine;
  if (choice.cpuOnly !== undefined) settings.translation.localEngine.cpuOnly = choice.cpuOnly;
}
