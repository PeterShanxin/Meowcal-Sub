import { t, subscribeLocale } from "../i18n/index";

function setupOverlayToolbar({ frame, onInteraction, onGeometryChange }) {
  const toolbar = document.getElementById("overlay-toolbar");
  const exitButton = document.getElementById("exit-translation-button");
  const error = document.getElementById("overlay-toolbar-error");
  exitButton.removeAttribute("data-i18n-title");
  exitButton.title = t("overlay.exitHome");
  let exiting = false;
  let measuredSize;
  let exitErrorKey = null;

  const sync = () => {
    const inactive =
      !frame.classList.contains("visible") ||
      ["hidden", "faded", "exiting"].some((state) => frame.classList.contains(state));
    if (inactive) {
      toolbar.hidden = true;
      error.hidden = true;
      measuredSize = undefined;
      if (toolbar.contains(document.activeElement)) document.activeElement.blur();
    } else {
      const rect = frame.getBoundingClientRect();
      if (!measuredSize) toolbar.hidden = false;
      if (!toolbar.hidden)
        measuredSize = { width: toolbar.offsetWidth, height: toolbar.offsetHeight };
      const placement = window.OverlayGeometry.resolveToolbarPlacement(
        { x: rect.left, y: rect.top, width: rect.width, height: rect.height },
        { width: window.innerWidth, height: window.innerHeight },
        measuredSize,
      );
      toolbar.hidden = !placement;
      if (placement) {
        toolbar.style.left = `${placement.left}px`;
        toolbar.style.top = `${placement.top}px`;
      } else if (toolbar.contains(document.activeElement)) document.activeElement.blur();
    }
    onGeometryChange();
  };

  toolbar.addEventListener("mousedown", (event) => event.stopPropagation());
  toolbar.addEventListener("mouseenter", onInteraction);
  toolbar.addEventListener("focusin", onInteraction);
  window.addEventListener("focus", () => {
    if (!frame.classList.contains("visible") || frame.classList.contains("exiting")) return;
    frame.classList.remove("faded");
    sync();
    if (
      !toolbar.hidden &&
      !document.getElementById("settings-menu").classList.contains("visible")
    ) {
      document.getElementById("settings-button").focus();
      onInteraction();
    }
  });
  exitButton.addEventListener("click", async () => {
    if (exiting) return;
    exiting = true;
    exitButton.disabled = true;
    toolbar.setAttribute("aria-busy", "true");
    exitButton.title = t("overlay.exiting");
    exitErrorKey = null;
    error.hidden = true;
    sync();
    try {
      await window.TauriBridge.invoke("exit_translation");
    } catch (reason) {
      console.error("Failed to exit translation:", reason);
      exitErrorKey = String(reason).includes("Capture is still stopping")
        ? "overlay.captureStopping"
        : "overlay.exitFailed";
      error.textContent = t(exitErrorKey);
      error.hidden = false;
    } finally {
      exiting = false;
      exitButton.disabled = false;
      toolbar.setAttribute("aria-busy", "false");
      exitButton.title = t("overlay.exitHome");
      sync();
    }
  });

  new MutationObserver(sync).observe(frame, {
    attributes: true,
    attributeFilter: ["class", "style"],
  });
  window.TauriBridge.event.listen("overlay-exit-requested", () => exitButton.click());
  const observer = new ResizeObserver(() => requestAnimationFrame(sync));
  observer.observe(toolbar);
  observer.observe(frame);
  frame.addEventListener("transitionend", sync);
  window.addEventListener("resize", sync);
  const unsubscribeLocale = subscribeLocale(() => {
    exitButton.title = t(exiting ? "overlay.exiting" : "overlay.exitHome");
    if (exitErrorKey) error.textContent = t(exitErrorKey);
    measuredSize = undefined;
    sync();
  });
  window.addEventListener("pagehide", unsubscribeLocale, { once: true });
  sync();
}

window.OverlayToolbar = { setupOverlayToolbar };
