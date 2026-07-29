# Curated Translation Redesign: Proposed GitHub Backlog

**Date:** 2026-07-29
**Status:** Draft only; no GitHub writes authorized

## Structure

Create three linked epics. Product delivery owns visible outcomes. Maintainability owns code and verification foundations. Collaboration owns repository metadata and settings.

## Epic A: Make HY-MT a built-in local subtitle engine

### Problem

The current app exposes backend infrastructure and can reject correct translations, retry deterministic failures, and present source OCR as fallback translation. The HY-MT prototype proves feasibility but does not provide a complete install, repair, runtime ownership, or release lifecycle.

### Child issues

1. **A1 — Normalize Windows OCR language aliases**
   Reuse existing Issue #15. Add alias normalization, tests, and live `zh-Hans-CN -> zh-CN` verification.

2. **A2 — Make subtitle validation language-aware**
   Accept realistic Chinese/Japanese-to-English expansion; preserve empty, repetition, prompt-echo, and wrong-language guards.

3. **A3 — Separate retry classes and fallback states**
   Retry transient transport/runtime failures only. Replace `MockBackend` success semantics with explicit `sourceOnly`, `warming`, and unavailable states.

4. **A4 — Establish a curated translation evaluation set**
   Version representative clean, spaced, mixed-language, noisy, short, long, multiline, name, honorific, and idiom cases without production screen data.

5. **A5 — Define the versioned engine manifest**
   Architecture, platform selection, artifact metadata, licenses, hashes, minimum requirements, compatibility, and rollback contract.

6. **A6 — Implement install, adoption, verification, repair, and rollback**
   Reuse reviewed download/hash work from `feat/hymt-foundry`; keep last-known-good engine.

7. **A7 — Own the HY-MT runtime process**
   Exact process handle, dynamic validated loopback port, duplicate prevention, health, warm-up, restart, shutdown, and stale-process recovery.

8. **A8 — Replace Foundry setup with one engine setup flow**
   Normal mode shows product states and actions only. Developer mode contains unsupported endpoint/model controls.

9. **A9 — Fix startup window restoration**
   Hidden creation, validated DPI-aware bounds, one show, debounced persistence, missing-monitor recovery, and manual multi-monitor gate.

10. **A10 — Instrument and optimize the live pipeline**
    Stage timings, cancellation, stale results, dedupe, CPU/memory/process metrics, and ARM64/x64 budgets.

11. **A11 — Release closure for curated local translation**
    Clean install/repair/upgrade, sleep/resume, mixed DPI, exact process cleanup, support script, packaging, and 30-minute episode regression.

### Dependency order

```text
A1 OCR aliases ───────────────┐
A2 output validation ────────┼─> A4 evaluation set
A3 retry/fallback states ────┘        |
                                      v
A5 engine manifest -> A6 install/repair -> A7 runtime ownership -> A8 setup UX
                                                               -> A10 performance
A9 window lifecycle ------------------------------------------> A11 release closure
A4 evaluation set --------------------------------------------> A11 release closure
```

## Epic B: Raise repository readability and maintainability

Modeled on LexBridge #80, adapted for Rust/Tauri/vanilla JavaScript.

### Child issues

1. **B1 — Contributor standards and clean-checkout onboarding**
   Add `CONTRIBUTING.md`, canonical `docs/AGENT_GUIDE.md`, `docs/CODING_STANDARDS.md`, runtime/toolchain versions, CI-equivalent resource setup, and current commands.

2. **B2 — Establish one authoritative verification command**
   Root command runs Rust format/clippy/tests, frontend format/lint/tests, browser smoke, documentation checks, and baseline verifiers. CI invokes the same contract.

3. **B3 — Add frontend quality and test foundations**
   Format/lint contract, DOM-independent module tests, browser-mode smoke tests, and explicit Tauri-only test limits.

4. **B4 — Create architecture and maintainability baselines**
   Canonical architecture, module ownership, version ownership, measured hotspots, file-size ceiling, exception list, and downward-only ratchets.

5. **B5 — Extract Rust application and command boundaries**
   Thin IPC commands; separate app lifecycle, config migration, engine install, runtime, capture session, and diagnostics.

6. **B6 — Extract translation and pipeline boundaries**
   Separate prompt, transport, validation, retry, context, session orchestration, and overlay payload contracts.

7. **B7 — Extract main/setup frontend boundaries**
   Separate setup, engine state, session controls, OCR readiness, settings, and developer diagnostics.

8. **B8 — Extract overlay/capture frontend boundaries**
   Separate overlay session state, geometry, interaction, clipping, presentation, and bridge adapters.

9. **B9 — Add risk-based coverage and clean-checkout closure**
   Honest named coverage areas, exact floors proven to fail, clean worktree rehearsal, and documentation consistency.

### Dependency order

```text
B1 onboarding -> B2 unified verification -> B3 frontend foundation -> B4 boundaries/baseline
                                                              |
                                                              v
                              B5 Rust app lane ────────────────┐
                              B6 translation lane ────────────┼─> B9 closure
                              B7 main frontend lane ──────────┤
                              B8 overlay frontend lane ───────┘
```

Parallel B5-B8 work starts only after B4 assigns disjoint file and shared-contract ownership.

## Epic C: Standardize collaboration and change history

Modeled on LexBridge #107. This epic changes repository files and GitHub settings, not product behavior.

### Child issues

1. **C1 — Define commit and version contracts**
   Conventional commit types/scopes, release/version source ownership, examples, and reliable CI enforcement.

2. **C2 — Standardize PR titles and descriptions**
   Template covers intent, scope, validation, risk, manual gate, issue linkage, screenshots, and performance evidence when claimed.

3. **C3 — Make merge history and branch cleanup predictable**
   Select merge strategy, title inheritance, branch deletion, and admin recovery path.

4. **C4 — Add issue forms and a minimal label taxonomy**
   Bug, feature, maintenance, and manual-validation intake without label bureaucracy.

5. **C5 — Define ownership and review governance**
   CODEOWNERS, resolved-conversation policy, solo-maintainer bypass, and activation rule for independent human approval.

6. **C6 — Establish lightweight ADRs**
   ADR index, template, status transitions, supersession, and boundary between normative architecture and historical plans.

### Dependency order

```text
C1 commit/version contract -> C2 PR contract -> C3 merge settings
C4 issue forms -----------------------------------------------┐
C5 ownership/review ------------------------------------------┼-> consistency review
C6 ADRs ------------------------------------------------------┘
```

## Recommended first issue sequence

1. Track and approve ADR-0001 plus this backlog proposal.
2. Resolve local `main` versus `origin/main`.
3. Put the approved product spec under version control.
4. Deliver B1 and B2 before broad implementation.
5. Deliver A1-A3 as the first behavior-changing correctness group, with fresh manual validation.
6. Deliver A5-A7 before A8 so UI consumes a real engine state.
7. Begin B5-B8 only after product contracts and maintainability ratchets exist.

## Existing item disposition

### Issue #15

Keep open. Reuse as A1. Do not close until automated alias tests and live Windows OCR verification pass.

### `feat/hymt-foundry`

Keep local during design. Do not open a PR for the full branch. Extract bounded commits or patches into A5-A8 after architecture review.

### Old MeoCoSub2 plan

Classify as superseded historical concept under ADR-0001. Do not delete without explicit approval.

## External-write approval package

Before creating GitHub items, present:

- exact epic and child-issue titles;
- exact issue bodies;
- proposed labels;
- proposed milestone mapping;
- proposed repository setting changes;
- existing Issue #15 update text;
- branch/PR plan for the approved spec and Wave 0 documents.

No issue creation, comment, closure, push, merge, or settings change occurs until approved.
