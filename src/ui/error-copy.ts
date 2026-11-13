import { t, messages, getLocale } from "../i18n";
export type MessageAction = "selectRegion" | "repair";

export interface MessagePresentation {
  text: string;
  action: { kind: MessageAction; label: string } | null;
}

// Failures the user can fix from the main window, matched on the wording the
// backend and controller actually produce. Anything else is shown as reported:
// rewording a message nobody has classified would guess at its cause.
const knownErrors: ReadonlyArray<[RegExp, MessagePresentation]> = [
  [
    /no capture region set/i,
    {
      text: "Select the subtitle area before starting.",
      action: { kind: "selectRegion", label: "Select area" },
    },
  ],
  [
    /translation engine is not ready/i,
    {
      text: "The translation engine isn’t ready. Repair it to continue.",
      action: { kind: "repair", label: "Repair" },
    },
  ],
];

// Capture keeps running after these; they explain why subtitles may be missed.
const knownCaptureWarnings: ReadonlyArray<[RegExp, string]> = [
  [/GDI fallback/i, "Compatibility capture · Protected video may not be read"],
  [/OCR language .* is not installed/i, "Recognition language missing · Using Windows default"],
];

export function describeCaptureWarning(message: string): string {
  return localizeError(
    knownCaptureWarnings.find(([pattern]) => pattern.test(message))?.[1] ?? message,
  );
}

export function describeError(message: string): MessagePresentation {
  const known = knownErrors.find(([pattern]) => pattern.test(message));
  return known
    ? {
        text: t(known[1].text),
        action: known[1].action ? { ...known[1].action, label: t(known[1].action.label) } : null,
      }
    : { text: localizeError(message), action: null };
}

function localizeError(message: string): string {
  const recognition =
    /^Text recognition could not start: ([\s\S]*)\. Try the Compatibility profile in Settings\.$/.exec(
      message,
    );
  if (recognition)
    return t(
      "Text recognition could not start. Try the Compatibility profile in Settings. Technical details: {details}",
      { details: recognition[1] },
    );
  if (messages[message]) return t(message);
  if (getLocale() === "en" || Object.values(messages).some((value) => value["zh-CN"] === message))
    return message;
  return `${t("Something went wrong. Try again. Technical details:")} ${message}`;
}

export function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}
