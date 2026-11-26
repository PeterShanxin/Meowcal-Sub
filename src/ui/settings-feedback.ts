import type { AppSettings, UiSnapshot } from "./contracts";
import { mergeSettings } from "./settings-defaults";
import type { SettingsSaveState } from "./settings-writer";

/** A focus refresh may finish after the user has edited and saved a newer value. */
export function settingsAfterRefresh(
  before: UiSnapshot,
  current: UiSnapshot,
  stored: Partial<AppSettings> | null,
): Partial<UiSnapshot> {
  if (
    !stored ||
    before.settings !== current.settings ||
    before.settingsSave.kind !== "idle" ||
    current.settingsSave.kind !== "idle"
  )
    return {};
  const settings = mergeSettings(stored);
  if (current.settingsRead === "error") return { settings, settingsRead: "ready" };
  return { settings: { ...current.settings, overlay: settings.overlay } };
}

export function settingsSaveFeedback(
  snapshot: UiSnapshot,
  settingsSave: SettingsSaveState,
): Partial<UiSnapshot> {
  const saved =
    settingsSave.kind === "idle" &&
    snapshot.settingsSave.kind === "saving" &&
    (snapshot.screen === "settings" || snapshot.screen === "appearance") &&
    !snapshot.error &&
    (!snapshot.notice || snapshot.notice === "Settings saved");
  return { settingsSave, ...(saved ? { notice: "Settings saved" } : {}) };
}
