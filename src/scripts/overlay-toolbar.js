function setupOverlayToolbar({ frame, onInteraction, onGeometryChange }) {
  const toolbar = document.getElementById("overlay-toolbar");
  const exitButton = document.getElementById("exit-translation-button");
  const error = document.getElementById("overlay-toolbar-error");
  let exiting = false;

  const sync = () => {
    toolbar.hidden =
      !frame.classList.contains("visible") ||
      ["hidden", "faded", "exiting"].some((state) => frame.classList.contains(state));
    if (toolbar.hidden) {
      error.hidden = true;
      if (toolbar.contains(document.activeElement)) document.activeElement.blur();
    } else {
      const rect = frame.getBoundingClientRect();
      const { left, top } = window.OverlayGeometry.resolveToolbarPlacement(
        { x: rect.left, y: rect.top, width: rect.width, height: rect.height },
        { width: window.innerWidth, height: window.innerHeight },
        { width: toolbar.offsetWidth, height: toolbar.offsetHeight },
      );
      toolbar.style.left = `${left}px`;
      toolbar.style.top = `${top}px`;
    }
    onGeometryChange();
  };

  toolbar.addEventListener("mousedown", (event) => event.stopPropagation());
  toolbar.addEventListener("mouseenter", onInteraction);
  toolbar.addEventListener("focusin", onInteraction);
  exitButton.addEventListener("click", async () => {
    if (exiting) return;
    exiting = true;
    exitButton.disabled = true;
    toolbar.setAttribute("aria-busy", "true");
    exitButton.title = "Exiting translation…";
    error.hidden = true;
    sync();
    try {
      await window.TauriBridge.invoke("exit_translation");
    } catch (reason) {
      error.textContent = `Couldn’t exit translation. Try again. ${reason instanceof Error ? reason.message : String(reason)}`;
      error.hidden = false;
    } finally {
      exiting = false;
      exitButton.disabled = false;
      toolbar.setAttribute("aria-busy", "false");
      exitButton.title = "Exit translation and return to Home";
      sync();
    }
  });

  new MutationObserver(sync).observe(frame, {
    attributes: true,
    attributeFilter: ["class", "style"],
  });
  const observer = new ResizeObserver(sync);
  observer.observe(toolbar);
  observer.observe(frame);
  frame.addEventListener("transitionend", sync);
  window.addEventListener("resize", sync);
  sync();
}

window.OverlayToolbar = { setupOverlayToolbar };
