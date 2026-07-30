/* global module */

(function exposePipelineUpdate(root, factory) {
  const api = factory();
  if (typeof module === "object" && module.exports) {
    module.exports = api;
  }
  if (root) {
    root.PipelineUpdate = api;
  }
})(typeof globalThis !== "undefined" ? globalThis : this, () => {
  function position(payload) {
    const sessionId = Number(payload?.sessionId);
    const captureId = Number(payload?.captureId);
    if (!Number.isSafeInteger(sessionId) || !Number.isSafeInteger(captureId)) {
      return null;
    }
    return { sessionId, captureId };
  }

  function shouldAccept(previous, payload) {
    const next = position(payload);
    if (!next || !previous) return true;
    return (
      next.sessionId > previous.sessionId ||
      (next.sessionId === previous.sessionId && next.captureId > previous.captureId)
    );
  }

  return { position, shouldAccept };
});
