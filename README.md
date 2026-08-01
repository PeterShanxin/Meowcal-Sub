# Meowcal Sub

Meowcal Sub is a Windows desktop application that captures a subtitle region,
recognizes text with Windows OCR, translates locally, and displays the result in
a floating overlay.

Tencent HY-MT is the only supported translation engine in normal mode. The
current MVP provides app-managed download, integrity verification, startup,
health checks, repair, transactional promotion, last-known-good rollback,
shutdown, and a real sample translation. x64 evidence and the full episode
release gate remain open.

## Status

- Windows-only beta.
- Tauri 2, Rust, and vanilla HTML/CSS/JavaScript.
- Windows OCR and local translation; capture text stays on the device.
- Guided setup installs the supported HY-MT model and matching local runtime.
- Offline MT and Phi Silica backends described by older documentation were
  removed and are not supported.
- Known correctness and lifecycle work is tracked by
  [Product epic #26](https://github.com/PeterShanxin/Meowcal-Sub/issues/26).

Production logs record support codes, stage timings, IDs, and character counts,
not captured or translated subtitle text. Review any support bundle before
sharing because configuration and environment metadata are still included.

## Use the current beta

1. Launch Meowcal Sub.
2. Select the OCR source language and English target language.
3. Complete the current local translation setup.
4. Select the subtitle region.
5. Start translation and position the overlay.

If setup reports a support code, use **Install / Repair** and retain the code
when filing a problem.

The normal setup stays inside the app. For unattended recovery or support:

```powershell
.\scripts\support-engine.ps1 -Action InstallRepair -Unattended
.\scripts\support-engine.ps1 -Action Verify
.\scripts\support-engine.ps1 -Action CollectLogs
```

The support script uses the same embedded manifest, sizes, SHA-256 hashes,
architecture selection, preflight requirements, and rollback layout as the
app. It is not an alternative model-selection interface.

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
jobs. It verifies Rust formatting, lint, and tests; the engine support-script
contract; frontend formatting, lint, and unit tests; the real browser-to-Rust
health/settings bridge; and the locked frontend dependency graph.

The browser smoke does not prove screen capture, Windows OCR, the native
selector or overlay, tray behavior, installer behavior, or DPI/window
lifecycle. Those remain manual Windows gates.

Run the privacy-safe subtitle regression set without starting the model:

```powershell
npm run eval:subtitles
```

An opt-in live run validates the installed app-managed engine and writes a
report containing only case IDs, output shape, validator decisions, and
latency. See [evals/README.md](evals/README.md) for the command and grading
contract.

## Build installers

Build architecture-matched installers with the guarded release settings:

```powershell
.\scripts\build-package.ps1 -Architecture auto
```

Use `-Architecture x64` or `-Architecture arm64` to make the target explicit.
The ARM64 path serializes release compilation and disables LTO/stripping because
the current ARM64 Rust compiler can crash under the default parallel profile.
Generated bundles are under the selected Cargo target directory. Packaging is
not a release, and installer behavior still requires the manual Windows gate.
The `Windows Packages` workflow produces x64 bundles on a native Windows x64
runner; package generation does not replace x64 runtime or performance testing.

## Architecture

Current product path:

```text
capture -> preprocess -> Windows OCR -> normalize/dedupe
        -> local translation -> validate -> overlay
```

The target boundaries and rationale are recorded in:

- [approved product specification](docs/plans/2026-07-29-curated-local-translation-app-spec.md);
- [ADR-0001](docs/adr/0001-curated-local-translation-stack.md);
- [current architecture](docs/ARCHITECTURE.md);
- [maintainability baseline](docs/MAINTAINABILITY_BASELINE.md);
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
before sharing because they include configuration and environment metadata
even though subtitle text is excluded.

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
