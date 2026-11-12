export function setupProgress(line: string): string | null {
  const download = /^(Runtime|Model): (\d{1,3})%$/.exec(line);
  if (download) {
    return `Downloading ${download[1] === "Runtime" ? "engine" : "model"}: ${download[2]}%`;
  }
  if (line.startsWith("Checking Windows,")) return "Checking this PC…";
  if (line.startsWith("Downloading the translation runtime")) return "Downloading engine…";
  if (line.startsWith("Installing or repairing the translation runtime"))
    return "Installing engine…";
  if (line.startsWith("Downloading Tencent HY-MT")) return "Downloading model…";
  if (line.startsWith("Warming up and checking")) return "Checking the engine…";
  if (line.startsWith("Local Translation Engine verified")) return "Finishing setup…";
  return null;
}
