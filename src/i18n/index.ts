import chinese from "./en-zh.json";
import { overlayMessages } from "./overlay-messages";

export type UiLanguage = "system" | "en" | "zh-CN";
export type Locale = Exclude<UiLanguage, "system">;
export type Parameters = Record<string, string | number>;
type Message = { en: string; "zh-CN": string };

// English source messages are stable lookup keys; adapters use namespaced keys.
export const messages: Record<string, Message> = {
  ...Object.fromEntries(Object.entries(chinese).map(([en, zh]) => [en, { en, "zh-CN": zh }])),
  ...overlayMessages,
};

export function normalizePreference(value: unknown): UiLanguage {
  return value === "en" || value === "zh-CN" ? value : "system";
}

export function resolveLocale(preference: UiLanguage, languages: readonly string[]): Locale {
  if (preference !== "system") return preference;
  return /^zh(?:-|$)/i.test(languages[0] ?? "") ? "zh-CN" : "en";
}

let preference: UiLanguage = "system";
let pendingPreference = false;
let preferenceRevision = 0;
export function getPreferenceRevision(): number {
  return preferenceRevision;
}
let systemLanguages: readonly string[] =
  typeof navigator === "undefined" ? [] : navigator.languages;
let locale = resolveLocale(preference, systemLanguages);
const listeners = new Set<() => void>();

export function getLocale(): Locale {
  return locale;
}

export function getUiLanguage(): UiLanguage {
  return preference;
}

export function previewUiLanguage(value: UiLanguage): void {
  ++preferenceRevision;
  pendingPreference = true;
  setUiLanguage(value);
}

export function confirmUiLanguage(value: UiLanguage): void {
  pendingPreference = false;
  setUiLanguage(value);
}

export function acceptSavedUiLanguage(value: unknown, expectedRevision = preferenceRevision): void {
  if (!pendingPreference && expectedRevision === preferenceRevision)
    setUiLanguage(normalizePreference(value));
}

export function setUiLanguage(value: UiLanguage, languages = systemLanguages): void {
  preference = normalizePreference(value);
  systemLanguages = languages;
  const next = resolveLocale(preference, languages);
  const changed = locale !== next;
  locale = next;
  if (typeof document !== "undefined") {
    document.documentElement.lang = locale;
    document.title = t("Meowcal Sub");
  }
  if (changed) listeners.forEach((listener) => listener());
}

export function subscribeLocale(listener: () => void): () => void {
  listeners.add(listener);
  return () => listeners.delete(listener);
}

export function t(key: string, parameters: Parameters = {}): string {
  const message = messages[key]?.[locale] ?? key;
  return message.replace(/\{(\w+)\}/g, (match, name: string) =>
    Object.hasOwn(parameters, name) ? String(parameters[name]) : match,
  );
}
