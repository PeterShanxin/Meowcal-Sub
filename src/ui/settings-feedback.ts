import type { UiSnapshot } from "./contracts";
import type { SettingsSaveState } from "./settings-writer";

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
