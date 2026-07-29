# Frontend test boundary

- `unit/` covers DOM-independent presentation and state helpers.
- `browser/` runs the real static frontend against the Rust HTTP backend through
  `TauriBridge`.

The browser smoke proves API health, settings/readiness bridge behavior, and
graceful `501` handling. It does not prove Windows screen capture, OCR,
selection, native overlays, tray behavior, DPI handling, installers, or the
curated model runtime. Those remain explicit Windows integration/manual gates.
