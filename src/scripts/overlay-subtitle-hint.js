/* global module */

(function exposeOverlaySubtitleHint(root, factory) {
  const api = factory();
  if (typeof module === "object" && module.exports) {
    module.exports = api;
  }
  if (root) {
    root.OverlaySubtitleHint = api;
  }
})(typeof globalThis !== "undefined" ? globalThis : this, () => {
  let hideTimer = null;

  function backendDisplayName(id) {
    switch ((id || "").toLowerCase()) {
      case "local_engine":
        return "Local Translation Engine";
      case "mock":
        return "Passthrough";
      default:
        return id || "Unknown";
    }
  }

  function summarizeFoundryWarning(warning) {
    const raw = (warning || "").toString();
    const lower = raw.toLowerCase();
    if (lower.includes("recovered_after_retry")) {
      return { text: "Recovered after retry", severity: "ok" };
    }
    if (lower.includes("context_degraded")) {
      return { text: "Reduced context for speed", severity: "warn" };
    }
    if (lower.includes("timeout")) {
      return { text: "Timeout", severity: "warn" };
    }
    if (lower.includes("model not available") || lower.includes("no model available")) {
      return { text: "Model not running", severity: "warn" };
    }
    if (lower.includes("request failed") || lower.includes("error sending request")) {
      return { text: "Request failed", severity: "error" };
    }

    let cleaned = raw.replace(/^local_engine:\s*/i, "");
    cleaned = cleaned.replace(/^api error:\s*/i, "");
    cleaned = cleaned.replace(/https?:\/\/\S+/gi, "").trim();
    cleaned = cleaned.replace(/\s+/g, " ");
    if (cleaned.length > 90) cleaned = `${cleaned.slice(0, 87)}…`;
    return { text: cleaned || "Error", severity: "error" };
  }

  function summarizeGenericWarning(warning) {
    let cleaned = (warning || "").toString().trim();
    cleaned = cleaned.replace(/https?:\/\/\S+/gi, "").trim();
    cleaned = cleaned.replace(/\s+/g, " ");
    if (cleaned.length > 90) cleaned = `${cleaned.slice(0, 87)}…`;
    return cleaned;
  }

  function clearSubtitleHint(hintEl, hintTextEl) {
    if (hideTimer) {
      clearTimeout(hideTimer);
      hideTimer = null;
    }
    if (!hintEl) return;
    hintEl.classList.remove("visible", "hint-warn", "hint-error", "hint-ok");
    if (hintTextEl) hintTextEl.textContent = "";
  }

  function setSubtitleHint(hintEl, hintTextEl, text, severity, persist) {
    if (!hintEl || !hintTextEl) return;
    if (!text) {
      clearSubtitleHint(hintEl, hintTextEl);
      return;
    }
    if (hideTimer) {
      clearTimeout(hideTimer);
      hideTimer = null;
    }

    hintTextEl.textContent = text;
    hintEl.classList.add("visible");
    hintEl.classList.remove("hint-warn", "hint-error", "hint-ok");
    const normalizedSeverity = (severity || "warn").toLowerCase();
    if (normalizedSeverity === "ok") hintEl.classList.add("hint-ok");
    else if (normalizedSeverity === "error") hintEl.classList.add("hint-error");
    else hintEl.classList.add("hint-warn");

    if (!persist) {
      hideTimer = setTimeout(() => clearSubtitleHint(hintEl, hintTextEl), 4000);
    }
  }

  function updateSubtitleHint(hintEl, hintTextEl, backendUsed, warnings) {
    const backendId = (backendUsed || "").toString().toLowerCase();
    const list = Array.isArray(warnings)
      ? warnings.filter((warning) => typeof warning === "string")
      : [];
    const foundryWarnings = list.filter((warning) =>
      warning.toLowerCase().startsWith("local_engine:"),
    );
    const hadFoundryProblem = foundryWarnings.some((warning) => {
      const lower = warning.toLowerCase();
      return !lower.includes("recovered_after_retry") && !lower.includes("context_degraded");
    });
    const primaryFoundry = foundryWarnings[0];
    const primaryNonMock = list.find((warning) => !warning.toLowerCase().startsWith("mock:"));

    if (backendId === "local_engine") {
      if (list.length === 0) {
        clearSubtitleHint(hintEl, hintTextEl);
        return;
      }
      const summary = primaryFoundry ? summarizeFoundryWarning(primaryFoundry) : null;
      if (!summary?.text) {
        clearSubtitleHint(hintEl, hintTextEl);
        return;
      }
      setSubtitleHint(
        hintEl,
        hintTextEl,
        `${backendDisplayName(backendId)} · ${summary.text}`,
        summary.severity,
        false,
      );
      return;
    }

    let severity = "warn";
    let cause = "";
    if (primaryFoundry) {
      const summary = summarizeFoundryWarning(primaryFoundry);
      severity = summary.severity;
      cause = `Engine ${summary.text}`.trim();
    } else if (primaryNonMock) {
      cause = summarizeGenericWarning(primaryNonMock);
    }
    const labelPrefix = hadFoundryProblem ? "Fallback" : "Backend";
    const base =
      backendId === "mock"
        ? hadFoundryProblem
          ? "Fallback: OCR"
          : "OCR (no translation backend)"
        : `${labelPrefix}: ${backendDisplayName(backendId)}`;
    setSubtitleHint(hintEl, hintTextEl, cause ? `${base} · ${cause}` : base, severity, true);
  }

  return { clearSubtitleHint, setSubtitleHint, updateSubtitleHint };
});
