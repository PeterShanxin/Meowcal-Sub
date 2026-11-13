import { t, subscribeLocale } from "../i18n/index";

/* global module */

(function exposeOverlaySessionStatus(root, factory) {
  const api = factory();
  if (typeof module === "object" && module.exports) module.exports = api;
  if (root) root.OverlaySessionStatus = api;
})(typeof globalThis !== "undefined" ? globalThis : this, () => {
  function createSessionStatus() {
    let visible = false;
    let preparing = false;
    let waiting = false;
    let captureFailed = false;
    let engineFailed = false;
    let sessionInterrupted = false;
    let sessionId = 0;
    const positions = new Map();

    function accept(kind, payload) {
      if (!Number.isSafeInteger(payload?.sessionId)) return true;
      if (payload.sessionId < sessionId) return false;
      if (payload.sessionId > sessionId) {
        sessionId = payload.sessionId;
        positions.clear();
      }
      const previous = positions.get(kind) ?? -1;
      if (!Number.isSafeInteger(payload.captureId) || payload.captureId <= previous) return false;
      positions.set(kind, payload.captureId);
      return true;
    }

    return {
      visibility(next) {
        visible = next;
        preparing = next;
        captureFailed = engineFailed = sessionInterrupted = waiting = false;
      },
      translation(payload) {
        if (!accept("translation", payload)) return;
        // A late model result does not prove that source subtitles returned.
        if (payload.displayState === "translated") preparing = engineFailed = false;
        else if (payload.displayState === "warming") preparing = true;
        else if (payload.displayState === "noSubtitleText") preparing = false;
        else if (payload.displayState === "stopped") preparing = engineFailed = false;
        if (payload.warnings?.includes("local_engine: engine_recovery_failed")) engineFailed = true;
      },
      interrupted(payload) {
        if (!Number.isSafeInteger(payload?.sessionId) || payload.sessionId < sessionId) return;
        if (payload.sessionId > sessionId) positions.clear();
        sessionId = payload.sessionId;
        sessionInterrupted = true;
      },
      capture(payload) {
        if (payload.isError) captureFailed = true;
      },
      activity(payload) {
        if (!accept("activity", payload)) return;
        if (payload.state === "active") captureFailed = preparing = waiting = false;
        else if (payload.state === "waiting") {
          captureFailed = preparing = false;
          waiting = true;
        }
      },
      view() {
        if (!visible) return null;
        if (captureFailed || sessionInterrupted)
          return { text: t("overlay.interrupted"), error: true };
        if (engineFailed) return { text: t("overlay.engineStopped"), error: true };
        if (waiting) return { text: t("overlay.waiting"), error: false };
        if (preparing) return { text: t("overlay.preparing"), error: false };
        return null;
      },
    };
  }

  async function setupSessionStatus({ onChange, onInteraction, getRegion }) {
    const machine = createSessionStatus();
    const element = document.getElementById("overlay-session-status");
    const text = document.getElementById("overlay-session-status-text");
    const action = document.getElementById("overlay-session-status-action");
    let previous;
    function position() {
      if (!machine.view()) {
        element.hidden = true;
        onChange();
        return;
      }
      const region = getRegion();
      if (!region) {
        element.hidden = true;
        onChange();
        return;
      }
      const reserved = [
        {
          left: region.x,
          top: region.y,
          right: region.x + region.width,
          bottom: region.y + region.height,
        },
      ];
      for (const id of ["overlay-toolbar", "subtitle-container"]) {
        const control = document.getElementById(id);
        if (control && !control.hidden && !control.classList.contains("hidden"))
          reserved.push(control.getBoundingClientRect());
      }
      const left = Math.min(...reserved.map((rect) => rect.left));
      const top = Math.min(...reserved.map((rect) => rect.top));
      element.hidden = false;
      const placement = window.OverlayGeometry.resolveToolbarPlacement(
        {
          x: left,
          y: top,
          width: Math.max(...reserved.map((rect) => rect.right)) - left,
          height: Math.max(...reserved.map((rect) => rect.bottom)) - top,
        },
        { width: window.innerWidth, height: window.innerHeight },
        { width: element.offsetWidth, height: element.offsetHeight },
      );
      element.hidden = !placement;
      if (placement) {
        element.style.left = `${placement.left}px`;
        element.style.top = `${placement.top}px`;
      }
      onChange();
    }
    function render() {
      const view = machine.view();
      const key = view ? `${view.error}:${view.text}` : "hidden";
      if (key === previous) return;
      previous = key;
      element.hidden = view === null;
      element.setAttribute("role", view?.error ? "alert" : "status");
      element.classList.toggle("has-error", Boolean(view?.error));
      text.textContent = view?.text ?? "";
      action.hidden = !view?.error;
      position();
    }
    element.addEventListener("mouseenter", onInteraction);
    element.addEventListener("focusin", onInteraction);
    action.addEventListener("click", () =>
      document.getElementById("exit-translation-button").click(),
    );
    for (const [eventName, kind] of [
      ["capture-status", "capture"],
      ["subtitle-activity", "activity"],
      ["translation-interrupted", "interrupted"],
    ]) {
      await window.__TAURI__.event.listen(eventName, ({ payload }) => {
        machine[kind](payload);
        render();
      });
    }
    const geometry = new ResizeObserver(position);
    geometry.observe(element);
    geometry.observe(document.getElementById("capture-frame"));
    geometry.observe(document.getElementById("overlay-toolbar"));
    geometry.observe(document.getElementById("subtitle-container"));
    window.addEventListener("resize", position);
    const unsubscribeLocale = subscribeLocale(render);
    window.addEventListener("pagehide", unsubscribeLocale, { once: true });
    render();
    return {
      reposition: position,
      translation(payload) {
        machine.translation(payload);
        render();
      },
      visibility(visible) {
        machine.visibility(visible);
        render();
      },
    };
  }

  return { createSessionStatus, setupSessionStatus };
});
