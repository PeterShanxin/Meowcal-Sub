// CURATED TRANSLATION START HELPERS
// =============================================================================

(function () {
  "use strict";

  async function prepareManagedEngineForStart(status, startButton) {
    if (!status || !["notRunning", "preparing"].includes(status.phase)) {
      return status;
    }

    if (startButton) startButton.disabled = true;
    window.renderFoundryStatusChecking?.("Starting the private translation engine...");
    try {
      return await window.TauriBridge.invoke("make_engine_ready");
    } catch (error) {
      console.error("Failed to start the private translation engine:", error);
      return null;
    }
  }

  window.TranslationStart = { prepareManagedEngineForStart };
})();
