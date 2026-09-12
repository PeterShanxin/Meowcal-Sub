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

export function describeError(message: string): MessagePresentation {
  const known = knownErrors.find(([pattern]) => pattern.test(message));
  return known ? known[1] : { text: message, action: null };
}
