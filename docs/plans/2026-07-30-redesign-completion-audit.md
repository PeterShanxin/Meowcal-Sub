# Curated Redesign Completion Audit

**Audit date:** 2026-08-01

**Code baseline:** `f62de5a7c07718ae30e7db3d0760d130bb187efd`

**Approved contract:** `2026-07-29-curated-local-translation-app-spec.md`

This is a requirement-to-evidence audit, not a completion claim. A wave is
complete only when its full automated and named manual evidence exists.

## Current result

The repository has a working ARM64 HY-MT MVP, native package definitions, and a
substantially stronger foundation. The approved redesign is not complete. x64
runtime/performance validation, the full Windows matrix, and a 30-minute
episode regression remain release blockers.

## Wave evidence

| Wave | Status | Current evidence | Missing proof or implementation |
|---|---|---|---|
| 0 — Triage and baseline | Complete | Wave 0 baseline, approved product spec, ADRs, archived MeoCoSub2 plan, product and maintainability backlogs | Keep historical baseline documents clearly marked as historical |
| 1 — Repository foundation | Substantially complete | Normative contributor/agent/coding/architecture docs, unified verifier, frontend tests, browser smoke, dependency audit, maintainability and coverage ratchets | Governance templates/settings and final live-settings reconciliation |
| 2 — Critical correctness | Automated complete; manual gate open | Windows OCR aliases, language-aware HY-MT validation, deterministic rejection classification, explicit display states, representative dataset and live evaluation | Live OCR alias and overlay failure-path verification |
| 3 — Curated HY-MT engine | Automated complete; manual gate open | Typed embedded manifest, per-architecture selection, preflight, transactional install/repair/rollback, dynamic loopback port, exact process ownership, support script, licenses, and tests | Clean Windows install/adopt/repair/upgrade/restart/sleep rehearsal |
| 4 — Product UX | Normal-mode MVP complete; manual gate open | One private Tencent HY-MT setup, shared readiness, guided install/repair/sample translation, infrastructure choices removed | Keyboard/focus, first-run/returning-launch recording, and developer-mode decision |
| 5 — Lifecycle and performance | Automated complete; manual gate open | Hidden-first DPI-aware startup, session/capture IDs, cancellation, stale suppression, dedupe, stage timings, privacy-safe logs, and ARM64 live budget report | Whole-pipeline ARM64 capture evidence, x64 budget, monitor/sleep matrix |
| 6 — Modular decomposition | In progress | New engine, installer, validation, lifecycle, display and overlay-hint modules; downward file ratchets | `commands.rs`, `foundry_local.rs`, `manager.rs`, `main.js`, and remaining overlay/selector boundaries still exceed the reviewed ceiling |
| 7 — Release closure | Open | Clean automated gate and CI pass on the MVP; ARM64 direct model and tray-lifecycle smoke evidence; native x64 MSI/NSIS workflow succeeded on `f62de5a` (run `30691128927`) | Clean installer/adopt/repair/rollback/upgrade rehearsal, x64 runtime/performance evidence, full monitor and sleep/resume matrix, uninstall, and a fresh 30-minute episode regression after the final behavior change |

## Definition-of-done audit

| Requirement | Evidence state |
|---|---|
| Guided setup reaches a passing HY-MT sample | Proven on the ARM64 development installation; clean-install package rehearsal outstanding |
| Returning launch needs no command line | Implemented; packaged returning-launch manual proof outstanding |
| HY-MT is the only normal-mode engine | Proven by browser contract tests and UI source |
| Natural Chinese-to-English expansion is accepted | Proven by unit and live model sample |
| Deterministic rejection is not retried unchanged | Proven by manager unit test |
| Raw OCR is not mislabeled as translation | Proven by typed display-state tests; live failure-path manual proof outstanding |
| Main window shows once at valid final position | Hidden-before-show implemented; bounds validation and mixed-DPI manual proof outstanding |
| Exactly one app-owned runtime exists | Exact child ownership, dynamic port, concurrent-start tests, and ARM64 smoke pass; collision/restart/sleep-resume manual proof outstanding |
| ARM64 and x64 performance results | ARM64 live model budget report exists; whole-pipeline ARM64 and all x64 evidence missing |
| Unified verification matches CI and ratchets can fail | Current command/CI parity and negative ratchet contract tests pass |
| Core monoliths shrink or retain explicit exceptions | Enforced and shrinking, but decomposition is unfinished |
| Full episode regression passes | Missing and must be performed after the final visible change |
| GitHub backlog/governance matches the system | Backlog exists; governance files/settings and final issue reconciliation are incomplete |

## Next execution order

1. Run clean Windows install/adopt/repair/upgrade/restart/sleep and OCR alias
   rehearsals against the shipped engine manifest and support script.
2. Capture x64 runtime and performance evidence on a real x64 device; the
   native x64 package workflow is now proven, but ARM64 evidence must not be
   reused for runtime or performance claims.
3. Finish repository governance and bounded decomposition without weakening
   ratchets.
4. Run the final ARM64/x64 matrix and real 30-minute episode regression after
   the last visible behavior change.
