# Curated Redesign Completion Audit

**Audit date:** 2026-07-30  
**Code baseline:** `6205a953706ca447b482d96c6866c4205ad51d94`  
**Approved contract:** `2026-07-29-curated-local-translation-app-spec.md`

This is a requirement-to-evidence audit, not a completion claim. A wave is
complete only when its full automated and named manual evidence exists.

## Current result

The repository has a working ARM64 HY-MT MVP and a substantially stronger
foundation. The approved redesign is not complete. Engine upgrade safety,
pipeline performance evidence, x64 validation, the full Windows matrix, and a
30-minute episode regression remain release blockers.

## Wave evidence

| Wave | Status | Current evidence | Missing proof or implementation |
|---|---|---|---|
| 0 — Triage and baseline | Substantially complete | Wave 0 baseline, accepted ADR-0001, archived MeoCoSub2 plan, product and maintainability epics | Refresh performance baseline after the final pipeline lands |
| 1 — Repository foundation | Partial | Normative contributor/agent/coding/architecture docs, unified verifier, frontend tests, browser smoke, dependency audit, maintainability and coverage ratchets | ADR index/template, PR template, issue forms, CODEOWNERS, commit/version enforcement, documented live settings verification |
| 2 — Critical correctness | Automated MVP complete; manual gate open | Windows OCR aliases, language-aware HY-MT validation, deterministic rejection classification, explicit display states, representative subtitle unit cases | Live OCR alias verification and a versioned evaluation dataset with recorded outcomes |
| 3 — Curated HY-MT engine | MVP only | ARM64/x64 artifact metadata, resumable downloads, SHA-256 checks, app-owned child handle, loopback binding, health check, sample translation, repair path | Typed versioned manifest, compatibility/disk preflight, transactional staging, last-known-good rollback, dynamic reserved port, concurrent-start tests, crash/sleep recovery, matching support script, licensing review |
| 4 — Product UX | MVP only | Normal mode presents one private Tencent HY-MT engine, guided install/repair, shared readiness and real sample translation; infrastructure choices removed | Explicit developer mode contract, full state/action coverage, keyboard/focus validation, first-run recording |
| 5 — Lifecycle and performance | Partial | Main window starts hidden, saved geometry is applied before show, close hides to tray, runtime survives tray hiding | Monitor/DPI clamping, corrupt/off-screen recovery, debounced persistence, lazy initialization proof, stage timings, cancellation, stale-result suppression, frame dedupe, bounded queue, ARM64/x64 budget report |
| 6 — Modular decomposition | In progress | New engine, installer, validation, lifecycle, display and overlay-hint modules; downward file ratchets | `commands.rs`, `foundry_local.rs`, `manager.rs`, `main.js`, and remaining overlay/selector boundaries still exceed the reviewed ceiling |
| 7 — Release closure | Open | Clean automated gate and CI pass on the MVP; ARM64 direct model and tray-lifecycle smoke evidence | Clean installer/adopt/repair/rollback/upgrade rehearsal, x64 device evidence, full monitor and sleep/resume matrix, uninstall, and a fresh 30-minute episode regression after the final behavior change |

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
| Exactly one app-owned runtime exists | Exact child ownership and one ARM64 smoke pass exist; concurrent start, collision, restart, and sleep/resume proof outstanding |
| ARM64 and x64 performance results | ARM64 direct sample only; whole-pipeline ARM64 and all x64 evidence missing |
| Unified verification matches CI and ratchets can fail | Current command/CI parity and negative ratchet contract tests pass |
| Core monoliths shrink or retain explicit exceptions | Enforced and shrinking, but decomposition is unfinished |
| Full episode regression passes | Missing and must be performed after the final visible change |
| GitHub backlog/governance matches the system | Backlog exists; governance files/settings and final issue reconciliation are incomplete |

## Next execution order

1. Replace hardcoded artifact interpretation with a shipped, typed, validated
   engine manifest. The first release accepts manifest updates only through a
   signed application release; it does not trust a remotely refreshed unsigned
   manifest.
2. Make installation transactional with compatibility/free-space preflight,
   versioned staging, adoption, last-known-good promotion, rollback, and a
   matching PowerShell support path.
3. Centralize runtime state and add dynamic loopback-port reservation,
   concurrent-start convergence, collision classification, and bounded crash
   recovery.
4. Complete window-bounds correctness and privacy-safe pipeline
   instrumentation before making performance claims.
5. Finish repository governance and bounded decomposition without weakening
   ratchets.
6. Run the complete ARM64/x64 and real-episode release gate.
