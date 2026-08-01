# Curated Redesign Completion Audit

**Audit date:** 2026-08-01

**Product-code baseline:** `8c12412` (`fix: start managed engine without warmup prompt`)

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
| 2 — Critical correctness | Automated complete; manual gate open | Rust WinRT-boundary OCR alias normalization, frontend alias matching, language-aware HY-MT validation, deterministic rejection classification, explicit display states, representative dataset and live evaluation; live readiness smoke confirms installed `zh-Hans-CN` satisfies selected `zh-CN` without a false warning | Actual OCR recognition with Chinese subtitle text and overlay failure-path verification |
| 3 — Curated HY-MT engine | Automated complete; manual gate open | Typed embedded manifest, per-architecture selection, preflight, transactional install/repair/rollback, dynamic loopback port, exact process ownership, support script, licenses, and tests | Clean Windows install/adopt/repair/upgrade/restart/sleep rehearsal |
| 4 — Product UX | Normal-mode MVP complete; manual gate open | One private Tencent HY-MT setup, shared readiness, guided install/repair/sample translation, automatic start/warm-up from the normal Start Translation action when the managed files are present, fresh ARM64 managed-start smoke, infrastructure choices removed | Keyboard/focus, first-run/returning-launch recording, and developer-mode decision |
| 5 — Lifecycle and performance | Automated complete; manual gate open | Hidden-first DPI-aware startup, session/capture IDs, cancellation, stale suppression, dedupe, stage timings, privacy-safe logs, and explicit warm-up evaluation; comparable ARM64 runs measured auto-slot p50 841 ms / p95 4,091 ms versus single-slot p50 660 ms / p95 3,558 ms, with 33/33 quality passes | Whole-pipeline ARM64 capture evidence, x64 budget, monitor/sleep matrix, and a documented resolution for the remaining ARM64 p95 budget exception |
| 6 — Modular decomposition | In progress | New engine, installer, validation, lifecycle, display and overlay-hint modules; legacy compatibility boundary extracted; `commands.rs` now meets its 2,432-line ceiling | `foundry_local.rs`, `manager.rs`, `main.js`, and remaining overlay/selector boundaries still exceed the reviewed ceiling |
| 7 — Release closure | Open | Clean automated gate and CI pass on the MVP; fresh ARM64 Tauri window and managed-start smoke evidence; native x64 MSI/NSIS workflow succeeded on `2883966` (run `30699524713`) | Clean installer/adopt/repair/rollback/upgrade rehearsal, x64 runtime/performance evidence, full monitor and sleep/resume matrix, uninstall, and a fresh 30-minute episode regression after the final behavior change |

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
| ARM64 and x64 performance results | ARM64 direct-model before/after reports exist; single-slot runtime brings p50 within budget but p95 remains over budget; whole-pipeline ARM64 and all x64 runtime/performance evidence missing |
| Unified verification matches CI and ratchets can fail | Current command/CI parity and negative ratchet contract tests pass |
| Core monoliths shrink or retain explicit exceptions | Enforced and shrinking, but decomposition is unfinished |
| Full episode regression passes | Missing and must be performed after the final visible change |
| GitHub backlog/governance matches the system | Backlog exists; governance files/settings and final issue reconciliation are incomplete |

## Next execution order

1. Present three grounded Windows desktop UI directions and stop for the user's
   selection before changing production UI. Record the chosen direction as the
   acceptance target for the next visible wave.
2. Implement the selected UI direction and run a fresh focused Windows manual
   gate for first/returning launch, keyboard/focus, resize, setup, start/stop,
   and repair states.
3. Run clean Windows install/adopt/repair/upgrade/restart/sleep and OCR alias
   rehearsals against the shipped engine manifest and support script.
4. Capture x64 runtime and performance evidence on a real x64 device; the
   native x64 package workflow is now proven, but ARM64 evidence must not be
   reused for runtime or performance claims.
5. Finish repository governance and bounded decomposition without weakening
   ratchets.
6. Run the final ARM64/x64 matrix and real 30-minute episode regression after
   the last visible behavior change.
