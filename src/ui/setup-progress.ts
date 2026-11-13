import { t } from "../i18n";
export function setupProgress(line: string): string | null {
  const download = /^(Runtime|Model): (\d{1,3})%$/.exec(line);
  if (download) {
    return t(
      download[1] === "Runtime"
        ? "Downloading engine: {percent}%"
        : "Downloading model: {percent}%",
      { percent: download[2] },
    );
  }
  if (line.startsWith("Checking Windows,")) return t("Checking this PC…");
  if (line.startsWith("Downloading the translation runtime")) return t("Downloading engine…");
  if (line.startsWith("Installing or repairing the translation runtime"))
    return t("Installing engine…");
  if (line.startsWith("Downloading Tencent HY-MT")) return t("Downloading model…");
  if (line.startsWith("Warming up and checking")) return t("Checking the engine…");
  if (line.startsWith("Local Translation Engine verified")) return t("Finishing setup…");
  return null;
}
