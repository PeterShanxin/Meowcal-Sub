# Contributing to Meowcal Sub

**By intentionally submitting a contribution to this project, you agree to the
[Contributor License Agreement](CLA.md) for that contribution.** No per-pull-
request checkbox is required.

Meowcal Sub is a Windows desktop application for private, local subtitle
translation. It is an AGPL-3.0-only community project. The approved product
direction is a curated Tencent HY-MT engine, Windows OCR, and a Tauri 2/Rust
application. It is not a general local-model frontend.

Read these documents before changing the repository:

1. [`LICENSE`](LICENSE) and [`CLA.md`](CLA.md) for the community license and
   the contributor grant;
2. [`docs/AGENT_GUIDE.md`](docs/AGENT_GUIDE.md) for the working contract;
3. [`docs/CODING_STANDARDS.md`](docs/CODING_STANDARDS.md) for code and test rules;
4. [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md) and
   [`docs/MAINTAINABILITY_BASELINE.md`](docs/MAINTAINABILITY_BASELINE.md) for
   current ownership and enforced ratchets;
5. [`docs/CHANGE_CONTRACT.md`](docs/CHANGE_CONTRACT.md) for commit, version, and
   pull request rules;
6. [`docs/adr/README.md`](docs/adr/README.md) for accepted cross-cutting
   decisions and the ADR template;
7. current source, tests, and workflows for live behavior.

Historical documents under `docs/plans/` and `docs/archive/` provide context.
They are not normative and cannot override accepted ADRs or current guidance.

## Windows prerequisites

- Windows 11 is the supported development target. Windows 10 may work but is
  not a release claim.
- Git and PowerShell 7.
- Rust stable with `rustfmt` and `clippy`.
- Node.js 24 and npm 11. The supported major versions are declared in
  `package.json` and `.node-version`.
- Visual Studio with Desktop development with C++ and the Windows SDK. Build
  Tools, Community, Professional and Enterprise all work, and the installation
  does not have to be on `C:`.
- .NET 9 SDK when building the optional WinUI `OverlayHost`.

Both Windows ARM64 and x64 are in scope. `dev-tauri.cmd` discovers the installed
Visual Studio with `vswhere` and initializes it for the architecture of the host
it runs on, so ARM64 and x64 contributors both use the same launcher.

`scripts\dev-environment.ps1` makes those decisions and can be run on its own to
see what the launchers will use:

```powershell
pwsh -NoProfile -File scripts\dev-environment.ps1
```

It prints `MEOWCAL_RESOLVED_*` lines. Those names are deliberately not the
variables they become: the launchers clear them first, so a failed resolution
cannot be mistaken for a `CARGO_TARGET_DIR` or `MEOWCAL_VSDEVCMD` the shell was
already carrying.

Rust build output goes to `%LOCALAPPDATA%\meowcal-sub\cargo-build` by default,
which keeps it out of a OneDrive-synced checkout without naming a volume that a
given machine may not have. To build somewhere else - a faster disk, or an
existing build cache you want to keep - set `MEOWCAL_CARGO_TARGET_DIR`. An
explicit `CARGO_TARGET_DIR` overrides both.

## Isolated work

Use one Git worktree per branch. Do not modify another task's checkout or reuse
its build processes. A typical setup is:

```powershell
git fetch origin
git worktree add ..\meowcal-my-change -b fix/my-change origin/main
Set-Location ..\meowcal-my-change
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
iteration. The default `All` stage is the contributor handoff gate. It runs
against your host architecture; CI additionally repeats the Rust lint and test
stages for the other shipped architecture, so a green local run is the handoff
bar rather than a promise that CI will agree. The frontend stage installs
the locked npm graph, checks formatting and lint, runs DOM-independent unit
tests, exercises browser mode against the real Rust HTTP backend, and audits
dependencies for high-severity vulnerabilities. It also enforces production
file ceilings, the frontend warning budget, and explicitly scoped coverage
floors.

Rust dependency resolution is locked by `src-tauri/Cargo.lock`; verification
uses `--locked`. Frontend dependency resolution is locked by
`package-lock.json`; verification uses `npm ci`.

The browser smoke starts its own frontend and backend on ports the operating
system has just confirmed are free, so it never needs 3000 or 3001 to be
available and never asks you to stop something else that holds them. To pin it
to a known address while debugging, set `MEOWCAL_FRONTEND_PORT` and
`MEOWCAL_HTTP_PORT`; `.\dev-browser.cmd` reads the same two variables and
defaults to 3000 and 3001.

## License and contributor agreement

Community source is licensed under the GNU Affero General Public License
version 3 only. See [`LICENSE`](LICENSE). Commercial licensing is available
for organizations that require terms outside AGPL-3.0.

Acceptance is submission-based: the CLA terms in effect when you intentionally
submit material for inclusion govern that contribution. You keep copyright in
your work. The CLA grants the project owner perpetual copyright and patent
licenses, including sublicensing and relicensing rights needed to offer
separately commercially licensed versions. Individuals and entities use the
same grant. Viewing, starring, or forking the repository, opening an issue, or
joining a discussion does not by itself accept the CLA. Commercial use of the
public project under AGPL-3.0 does not require payment.

Name and logo use is separate from the code license. See
[`TRADEMARKS.md`](TRADEMARKS.md). Report vulnerabilities privately; see
[`SECURITY.md`](SECURITY.md).

## Open distribution gates

The Tencent HY-MT model the app can download is under Tencent's community
license, not AGPL. Redistribution review of that upstream license remains
open (`distributionReview: requiredBeforeRelease` in
[`config/engine-manifest.v1.json`](config/engine-manifest.v1.json)). Do not
treat this repository going public as completing that gate.

## Continuous integration

Every pull request, including forks and Dependabot, gets the hosted Windows
verify gate on `windows-11-arm`. Those jobs install the toolchain and run
`scripts/verify.ps1`. They are the merge gate. They use read-only contents
permission and do not receive signing or legacy-bridge secrets.

The owner's self-hosted runners are not that gate. `meowcal-ci` is a
maintainer-only `workflow_dispatch` path for real Snapdragon/Adreno hardware.
Packaging stays on the labeled self-hosted package runners. Those privileged
jobs run only when the GitHub actor is `PeterShanxin` or `ianmeowmeow`. Host
trust is those named logins, not write access in general. A fork PR,
Dependabot, or any other login does **not** schedule them.

The **Change Contract** check stays on hosted Ubuntu.

Do **not** register your personal computer as a runner. A runner executes
repository code directly on the host, so attaching one is a decision the
repository owner makes, not a way to speed up your own pull request.
`scripts/verify.ps1` is your local equivalent of the hosted gate.

If no self-hosted packaging or hardware runner is online, that job queues
rather than failing over to hosted Windows. That is intentional. See
[`docs/SELF_HOSTED_RUNNERS.md`](docs/SELF_HOSTED_RUNNERS.md).

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

## Reporting issues

Four forms cover reported work: bug report, feature request,
maintenance/refactor, and documentation. Each applies its own `type:` label, so
an issue starts correctly classified. They ask only for what triage actually
needs - version, Windows build and architecture, expected versus actual,
reproduction, scope and non-goals, and whether visible behavior changes.

Blank issues stay available. Most issues here are opened by the maintainer as
epics, waves, audits, and closeout records, and those fit no form; turning blank
issues off would tax the common case to tidy the rare one.

**Never paste captured subtitle text, screenshots of what you were watching, or
raw logs containing them.** This application reads text off your screen. Timings,
error codes, engine state, and file paths are the parts that help.

### Labels

Five dimensions, and nothing else:

| Prefix | Answers | Count |
|---|---|---|
| `type:` | `bug`, `feature`, `maintenance`, `docs` | exactly one; the form applies it |
| `area:` | which part of the system | one, or two when a change genuinely spans a boundary |
| `priority:` | `p0` blocks the primary user outcome or a release gate, `p1` important next-wave work, `p2` valuable follow-up | exactly one |
| `epic` | a parent issue coordinating bounded children | as needed |
| `gate:manual-validation` | cannot close without fresh manual Windows evidence | as needed |

The ten `area:` values, and where the boundary between neighbours falls:

| Label | Covers |
|---|---|
| `area:engine` | HY-MT model, install, manifest, engine runtime and process lifecycle |
| `area:ocr` | Windows OCR, language capability, preprocessing, recognition |
| `area:translation` | prompt, model request, response parsing, validation, output quality |
| `area:pipeline` | capture-to-overlay orchestration: retry, dedupe, queue, session state |
| `area:lifecycle` | startup, shutdown, window, tray, sleep, resume, persistence |
| `area:ui` | main, setup, selector, and overlay frontend behavior |
| `area:architecture` | module boundaries and structural ownership |
| `area:repository` | CI, tooling, testing, contributor workflow, maintainability |
| `area:governance` | GitHub metadata, review, ownership, history, ADR policy |
| `area:release` | packaging, upgrade, the manual release gate, release evidence |

Capture *timing and sequencing* is `area:pipeline`; the capture surface a user
drags is `area:ui`. Whether the engine starts is `area:engine`; whether its
answer is usable is `area:translation`.

Labels deliberately do not encode wave or status: milestones own sequencing and
checklists own completion. A new label means the taxonomy is missing a
dimension - not that one issue is unusual.

## Changes and pull requests

[`docs/CHANGE_CONTRACT.md`](docs/CHANGE_CONTRACT.md) is the single contract for
commit messages, product version ownership, pull request titles and
descriptions, merge history, and how ownership and review work. CI enforces its
mechanical parts in the **Change Contract** check.

`main` takes changes only through a pull request, with `Lint & Format`, `Tests`,
`Frontend & Browser`, and `Change Contract` green and every review conversation
resolved. Every pull request, including forks and Dependabot, takes those
GitHub-hosted checks. Privileged packaging and real Snapdragon/Adreno hardware
validation remain trusted-admin-only and are not pull-request jobs. Required
approvals are zero while this repository is effectively solo-maintained; the
contract says when that changes and who may bypass a rule to recover.

Merging always produces a merge commit - squash and rebase are disabled - and
the merged branch is deleted automatically. Your local branch and worktree are
untouched; `git fetch --prune` clears the stale remote pointers.

Read it before your first pull request. These are the rules it deliberately
leaves to review rather than mechanizing:

- Keep one bounded concern per pull request.
- Do not mix a visible product change with broad behavior-preserving extraction.
- Do not claim a performance improvement without comparable before/after
  measurements.
- Do not push, merge, close issues, or change repository settings without the
  authority required by the current task.

Check a branch before opening a pull request:

```powershell
npm run commits:check
```

That run is local feedback. CI is the gate.
