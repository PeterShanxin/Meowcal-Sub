(function exposeBackendStatusPresentation(root, factory) {
  const presentation = factory();

  if (typeof module === "object" && module.exports) {
    module.exports = presentation;
  }

  root.BackendStatusPresentation = presentation;
})(globalThis, function createBackendStatusPresentation() {
  function formatReadyState(readyState) {
    switch (readyState) {
      case "ready":
        return { label: "Ready", className: "ready" };
      case "notReady":
        return { label: "Not Ready", className: "not-ready" };
      case "notSupported":
        return { label: "Not Supported", className: "not-supported" };
      case "error":
        return { label: "Error", className: "error" };
      default:
        return { label: "Unknown", className: "error" };
    }
  }

  function formatFoundryPhase(phase) {
    switch (phase) {
      case "ready":
        return { label: "Ready", className: "ready" };
      case "unchecked":
        return { label: "Not checked", className: "unchecked" };
      case "preparing":
        return { label: "Preparing", className: "preparing" };
      case "notRunning":
      case "notrunning":
        return { label: "Not Running", className: "not-ready" };
      case "noModels":
      case "nomodels":
        return { label: "No Models", className: "not-ready" };
      case "notInstalled":
      case "notinstalled":
        return { label: "Not Installed", className: "not-supported" };
      case "error":
        return { label: "Error", className: "error" };
      default:
        return { label: "Unknown", className: "error" };
    }
  }

  return Object.freeze({
    formatFoundryPhase,
    formatReadyState,
  });
});
