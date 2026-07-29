# Meowcal Sub

Meowcal Sub is a Windows desktop application that captures a subtitle region,
recognizes text with Windows OCR, translates locally, and displays the result in
a floating overlay.

The approved redesign makes Tencent HY-MT the only supported translation engine
in normal mode. The app-managed download, repair, rollback, and guided setup are
being delivered through tracked waves; the current beta does not yet satisfy
that complete product promise.

## Status

- Windows-only beta.
- Tauri 2, Rust, and vanilla HTML/CSS/JavaScript.
- Windows OCR and local translation; capture text stays on the device.
- Current generic Foundry-style setup is transitional.
- Offline MT and Phi Silica backends described by older documentation were
  removed and are not supported.
- Known correctness and lifecycle work is tracked by
  [Product epic #26](https://github.com/PeterShanxin/Meowcal-Sub/issues/26).

Do not use this beta for sensitive material unless you also accept that debug
logs can contain OCR and translated text. Production privacy-safe logging is
part of the redesign.

## Use the current beta

1. Launch Meowcal Sub.
2. Select the OCR source language and English target language.
3. Complete the current local translation setup.
4. Select the subtitle region.
5. Start translation and position the overlay.

The normal-mode HY-MT guided setup described in the approved specification is
not complete yet. Follow the open epic rather than relying on old backend setup
instructions.

## Development prerequisites

- Windows 11;
- Rust stable with `rustfmt` and `clippy`;
- Node.js 24 with npm 11;
- Visual Studio 2022 Build Tools with C++ and Windows SDK;
- .NET 9 SDK for the optional WinUI `OverlayHost`.

See [CONTRIBUTING.md](CONTRIBUTING.md) for ARM64/x64 setup, worktree rules, and
the complete validation contract.

## Run in Tauri

The helper builds architecture-matched overlay resources, initializes the
current ARM64 Visual Studio environment, and launches Tauri:

```powershell
.\dev-tauri.cmd
```

For another architecture, build the overlay explicitly from the matching
Developer PowerShell and launch Tauri:

```powershell
.\scripts\build-overlayhost.ps1 -Architecture x64
npx --yes @tauri-apps/cli@2 dev
```

## Browser development mode

Browser mode connects the static frontend to the real Rust HTTP backend:

```powershell
.\dev-browser.cmd
```

Open `http://localhost:3000`. Browser mode cannot validate screen capture,
Windows OCR, the area selector, native overlay behavior, tray behavior, or
window/DPI lifecycle.

## Automated checks

From a clean checkout:

```powershell
.\scripts\verify.ps1
```

This is the command used by contributors and split across the current GitHub
jobs. It verifies Rust formatting, lint, and tests; frontend formatting, lint,
and unit tests; the real browser-to-Rust health/settings bridge; and the locked
frontend dependency graph.

The browser smoke does not prove screen capture, Windows OCR, the native
selector or overlay, tray behavior, installer behavior, or DPI/window
lifecycle. Those remain manual Windows gates.

## Build installers

Build real overlay resources before packaging:

```powershell
.\scripts\build-overlayhost.ps1 -Architecture auto
npx --yes @tauri-apps/cli@2 build --bundles nsis,msi
```

Generated bundles are under `src-tauri/target/release/bundle/`. Packaging is not
a release, and installer behavior still requires the manual Windows gate.

## Architecture

Current product path:

```text
capture -> preprocess -> Windows OCR -> normalize/dedupe
        -> local translation -> validate -> overlay
```

The target boundaries and rationale are recorded in:

- [approved product specification](docs/plans/2026-07-29-curated-local-translation-app-spec.md);
- [ADR-0001](docs/adr/0001-curated-local-translation-stack.md);
- [Wave 0 baseline](docs/plans/2026-07-29-wave-0-baseline.md).

The implementation is intentionally migrating through small reviewable changes,
not a wholesale rewrite.

## Logs and troubleshooting

Session logs are written under:

```text
%APPDATA%\com.meowcal.sub\logs\
```

When reporting a problem, include the app commit/version, Windows build,
architecture, exact reproduction steps, and the relevant log. Review logs
before sharing because the current beta can record OCR and translated text.

Common build failures:

- missing Tauri bundle resources: run
  `.\scripts\prepare-validation-resources.ps1` for automated checks or build the
  real `OverlayHost` for development/package work;
- locked `meowcal-sub.exe`: close the exact repository-built app process and
  retry;
- missing C++/Windows SDK tools: use the matching Visual Studio Developer
  PowerShell.

## Contributing

Start with [CONTRIBUTING.md](CONTRIBUTING.md) and
[docs/AGENT_GUIDE.md](docs/AGENT_GUIDE.md). User-visible changes require fresh
manual Windows validation. Performance claims require before/after evidence.

## License

MIT.
