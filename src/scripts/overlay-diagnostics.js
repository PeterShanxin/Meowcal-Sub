/* global module */

// Builds the text of the overlay diagnostics panel.
//
// The panel is the only place a viewer can see why a line is slow or wrong
// while they are actually watching something. It therefore reports the two
// things that cannot be reconstructed afterwards: where the wait went, and what
// OCR actually read before the translator saw it.
(function exposeOverlayDiagnostics(root, factory) {
  const api = factory();
  if (typeof module === "object" && module.exports) {
    module.exports = api;
  }
  if (root) {
    root.OverlayDiagnostics = api;
  }
})(typeof globalThis !== "undefined" ? globalThis : this, () => {
  function engineLabel(backendUsed) {
    if (!backendUsed) {
      // Lifecycle notices run no backend at all; naming one reads as a
      // fallback that never happened.
      return "";
    }
    return backendUsed === "mock" ? "mock (source only)" : backendUsed;
  }

  function latencyLine(modelMs, totalMs) {
    const total = Number(totalMs) || 0;
    const model = Number(modelMs) || 0;
    if (total <= 0) return "";
    // Model time is called out separately because it is the part no amount of
    // pipeline tuning can remove - it is the translator thinking.
    return model > 0 ? `Latency: ${total}ms (model ${model}ms)` : `Latency: ${total}ms`;
  }

  /**
   * @param {{ state?: string, backendUsed?: string, warnings?: string[],
   *           timestamp?: number, modelMs?: number, totalMs?: number,
   *           source?: string, now?: string }} update
   * @returns {string} newline-separated panel text
   */
  function buildDiagnosticsText(update) {
    const warnings = Array.isArray(update?.warnings) ? update.warnings.filter(Boolean) : [];
    const status = [
      `State: ${update?.state || "unknown"}`,
      engineLabel(update?.backendUsed),
      warnings.join(", "),
      update?.now || "",
    ].filter(Boolean);

    const source = typeof update?.source === "string" ? update.source.trim() : "";
    return [
      status.join(" · "),
      latencyLine(update?.modelMs, update?.totalMs),
      source ? `Source: ${source}` : "",
    ]
      .filter(Boolean)
      .join("\n");
  }

  return { buildDiagnosticsText };
});
