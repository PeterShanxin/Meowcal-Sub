/* global module */

(function exposeWizardState(root, factory) {
  const api = factory();
  if (typeof module === "object" && module.exports) {
    module.exports = api;
  }
  if (root) {
    root.WizardState = api;
  }
})(typeof globalThis !== "undefined" ? globalThis : this, () => {
  function supportCode(error) {
    const match = String(error?.message || error || "").match(/\bENGINE_[A-Z0-9_]+\b/);
    return match ? match[0] : "ENGINE_SETUP_FAILED";
  }

  function isReady(status) {
    return Boolean(status?.serviceRunning && status?.phase === "ready");
  }

  function progressMessage(line) {
    const text = String(line || "").trim();
    if (!text) return "Working…";
    if (text.includes("Checking Windows")) return "Checking this PC…";
    if (text.includes("Downloading Tencent")) return "Downloading translation engine…";
    if (text.includes("Downloading the translation runtime")) {
      return "Downloading engine support files…";
    }
    if (text.includes("Installing or repairing")) return "Installing translation engine…";
    if (text.includes("Warming up")) return "Starting and testing translation…";
    if (text.includes("restored the last known-good")) return "Restoring the working engine…";
    if (text.includes("verified")) return "Verifying translation engine…";
    return text;
  }

  return { isReady, progressMessage, supportCode };
});
