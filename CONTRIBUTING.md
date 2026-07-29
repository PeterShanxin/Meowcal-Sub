# Contributing to Meowcal Sub

Meowcal Sub is a Windows desktop application for private, local subtitle
translation. The approved product direction is a curated Tencent HY-MT engine,
Windows OCR, and a Tauri 2/Rust application. It is not a general local-model
frontend.

Read these documents before changing the repository:

1. [`docs/AGENT_GUIDE.md`](docs/AGENT_GUIDE.md) for the working contract;
2. [`docs/CODING_STANDARDS.md`](docs/CODING_STANDARDS.md) for code and test rules;
3. [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md) and
   [`docs/MAINTAINABILITY_BASELINE.md`](docs/MAINTAINABILITY_BASELINE.md) for
   current ownership and enforced ratchets;
4. [`docs/adr/README.md`](docs/adr/README.md) for accepted cross-cutting
   decisions and the ADR template;
5. current source, tests, and workflows for live behavior.

Historical documents under `docs/plans/` and `docs/archive/` provide context.
They are not normative and cannot override accepted ADRs or current guidance.

## Windows prerequisites

- Windows 11 is the supported development target. Windows 10 may work but is
  not a release claim.
- Git and PowerShell 7.
- Rust stable with `rustfmt` and `clippy`.
- Node.js 24 and npm 11. The supported major versions are declared in
  `package.json` and `.node-version`.
- Visual Studio 2022 Build Tools with Desktop development with C++ and the
  Windows SDK.
- .NET 9 SDK when building the optional WinUI `OverlayHost`.

Both Windows ARM64 and x64 are in scope. Use a native developer shell for the
target architecture. `dev-tauri.cmd` currently initializes the ARM64 Visual
Studio toolchain; x64 contributors should use an x64 Developer PowerShell until
the launcher is generalized.

## Isolated work

Use one Git worktree per branch. Do not modify another task's checkout or reuse
its build processes. A typical setup is:

```powershell
git fetch origin
git worktree add D:\CodexWorktrees\meowcal-my-change -b fix/my-change origin/main
Set-Location D:\CodexWorktrees\meowcal-my-change
```

Preserve unrelated and untracked files. Stage explicit paths instead of the
entire worktree when unrelated changes are present.

## Prepare a clean checkout

Tauri validates declared bundle resources during Rust checks. Normal CI does
not need the real WinUI executable, so both CI and local validation use the
same non-destructive helper:

```powershell
.\scripts\prepare-validation-resources.ps1
```

The helper creates ignored, empty files only when the corresponding resource
is absent. It never overwrites a real `OverlayHost` build. These placeholders
are suitable for format, lint, and automated tests only.

For Tauri development or release packaging, build the real architecture-matched
resources first:

```powershell
.\scripts\build-overlayhost.ps1 -Architecture auto
```

## Verification

Run the authoritative repository gate from the root:

```powershell
.\scripts\verify.ps1
```

Use `-Stage Lint`, `-Stage Test`, or `-Stage Frontend` only for focused local
iteration. The default `All` stage is the contributor handoff gate and equals
the union of the current required GitHub checks. The frontend stage installs
the locked npm graph, checks formatting and lint, runs DOM-independent unit
tests, exercises browser mode against the real Rust HTTP backend, and audits
dependencies for high-severity vulnerabilities. It also enforces production
file ceilings, the frontend warning budget, and explicitly scoped coverage
floors.

Rust dependency resolution is locked by `src-tauri/Cargo.lock`; verification
uses `--locked`. Frontend dependency resolution is locked by
`package-lock.json`; verification uses `npm ci`.

## Manual Windows validation

Automated tests do not prove screen capture, Windows OCR installation, overlay
placement, DPI behavior, runtime lifecycle, or installer behavior.

Any user-visible behavior change resets the relevant manual gate. Record:

- commit tested;
- architecture and Windows build;
- exact scenario;
- expected and observed result;
- logs, screenshots, or timings when relevant.

Do not close a `gate:manual-validation` issue from unit tests alone.

## Changes and pull requests

- Keep one bounded concern per pull request.
- Use a Conventional Commit style subject such as `fix:`, `feat:`, `docs:`,
  `test:`, `refactor:`, `perf:`, `build:`, or `ci:`.
- Explain intent, scope, non-goals, validation, risk, rollback, manual evidence,
  and linked issue.
- Do not claim a performance improvement without before/after measurements.
- Do not mix a visible product change with broad behavior-preserving extraction.
- Do not push, merge, close issues, or change repository settings without the
  authority required by the current task.

## Version ownership

`src-tauri/tauri.conf.json` is the product version record. The matching copies
in `package.json` and `src-tauri/Cargo.toml` must change in the same pull
request. Issue #37 owns automated drift prevention. Release automation is out
of scope until separately approved.
