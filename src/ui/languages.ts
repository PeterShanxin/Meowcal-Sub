export const languages = [
  { value: "zh-CN", label: "Chinese (Simplified)" },
  { value: "zh-TW", label: "Chinese (Traditional)" },
  { value: "ja-JP", label: "Japanese" },
  { value: "ko-KR", label: "Korean" },
  { value: "en-US", label: "English (US)" },
  { value: "es-ES", label: "Spanish" },
  { value: "fr-FR", label: "French" },
  { value: "de-DE", label: "German" },
] as const;

export function languageLabel(value: string): string {
  return languages.find((language) => language.value === value)?.label ?? value;
}

export interface LanguagePair {
  sourceLanguage: string;
  targetLanguage: string;
}

function fallbackLanguage(language: string): string {
  return language === "en-US" ? "zh-CN" : "en-US";
}

export function ensureDistinctLanguagePair<T extends LanguagePair>(pair: T): T {
  if (pair.sourceLanguage !== pair.targetLanguage) return pair;
  return { ...pair, targetLanguage: fallbackLanguage(pair.sourceLanguage) };
}

export function applyLanguageSelection<T extends LanguagePair>(
  pair: T,
  kind: "source" | "target",
  value: string,
): T {
  const next = {
    ...pair,
    [kind === "source" ? "sourceLanguage" : "targetLanguage"]: value,
  };
  if (next.sourceLanguage !== next.targetLanguage) return next;

  return {
    ...next,
    [kind === "source" ? "targetLanguage" : "sourceLanguage"]: fallbackLanguage(value),
  };
}
