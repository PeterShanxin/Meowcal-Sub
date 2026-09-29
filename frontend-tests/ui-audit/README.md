# UI audit preview

```sh
npm ci
npm run dev:ui-audit
```

Open the printed local address with `/audit` appended. This runs the actual
frontend against an in-memory HTTP fixture. The fixture can simulate successful,
failed, slow, and unavailable settings storage. It never downloads a model,
captures the screen, or performs OCR or translation. Restarting the server resets
its settings.

Use the viewport selector for 1000 × 700, the native minimum 560 × 430, and
390 × 844 or 320 × 740 reflow checks. These are iframe viewport sizes, not touch,
mobile Safari, or native DPI emulation.

The regression cases in `frontend-tests/browser/ui-audit.spec.ts` run with the
repository's browser suite on Windows. They exercise UI failures using routed
responses; the other browser smoke tests retain the real Rust bridge. Run
`scripts/verify.ps1` and the native Windows manual gate before releasing changes.
