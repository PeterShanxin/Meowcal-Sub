/* global module */

// Overlay settings popup wiring.
//
// The overlay window is click-through outside its clipped surfaces, so a click
// on the desktop never reaches this document. The popup must therefore own an
// explicit close affordance (close button, Escape, and the gear toggle) rather
// than relying on an outside-click handler alone.
(function exposeOverlaySettingsMenu(root) {
  function setMenuOpen(menu, open) {
    menu.classList.toggle("visible", open);
    menu.classList.toggle("hidden", !open);
  }

  // options: { button, menu, closeButton, fontSizeSlider, fontSizeDisplay,
  //            diagnosticsToggle, initialFontSize, initialDiagnostics,
  //            onOpenChange, onFontSize, onDiagnostics, onCommit }
  function setupSettingsMenu(options) {
    const {
      button,
      menu,
      closeButton,
      fontSizeSlider,
      fontSizeDisplay,
      diagnosticsToggle,
      initialFontSize,
      initialDiagnostics,
      onOpenChange,
      onFontSize,
      onDiagnostics,
      onCommit,
    } = options;

    if (!button || !menu) return null;

    let open = false;

    if (fontSizeSlider) fontSizeSlider.value = String(initialFontSize);
    if (fontSizeDisplay) fontSizeDisplay.textContent = `${initialFontSize}px`;
    if (diagnosticsToggle) diagnosticsToggle.checked = initialDiagnostics === true;

    const applyOpen = (next) => {
      if (open === next) return;
      open = next;
      setMenuOpen(menu, open);
      onOpenChange(open);
    };

    // Keep the capture frame from starting a drag when the gear is pressed.
    button.addEventListener("mousedown", (event) => {
      event.stopPropagation();
      event.preventDefault();
    });

    button.addEventListener("click", (event) => {
      event.stopPropagation();
      event.preventDefault();
      applyOpen(!open);
    });

    if (closeButton) {
      closeButton.addEventListener("mousedown", (event) => event.stopPropagation());
      closeButton.addEventListener("click", (event) => {
        event.stopPropagation();
        event.preventDefault();
        applyOpen(false);
      });
    }

    // Only reaches us when the click lands on an interactive overlay surface.
    document.addEventListener("click", (event) => {
      if (!open) return;
      if (menu.contains(event.target) || event.target === button) return;
      applyOpen(false);
    });

    document.addEventListener("keydown", (event) => {
      if (open && event.key === "Escape") {
        event.preventDefault();
        applyOpen(false);
      }
    });

    if (fontSizeSlider) {
      fontSizeSlider.addEventListener("input", (event) => {
        const value = Number.parseInt(event.target.value, 10);
        if (!Number.isFinite(value)) return;
        if (fontSizeDisplay) fontSizeDisplay.textContent = `${value}px`;
        onFontSize(value);
      });
      fontSizeSlider.addEventListener("change", () => onCommit());
    }

    if (diagnosticsToggle) {
      diagnosticsToggle.addEventListener("change", (event) => {
        onDiagnostics(event.target.checked === true);
        onCommit();
      });
    }

    button.style.pointerEvents = "auto";
    menu.style.pointerEvents = "auto";

    return { isOpen: () => open, close: () => applyOpen(false) };
  }

  const api = { setupSettingsMenu };
  if (typeof module === "object" && module.exports) {
    module.exports = api;
  }
  if (root) {
    root.OverlaySettingsMenu = api;
  }
})(typeof globalThis !== "undefined" ? globalThis : this);
