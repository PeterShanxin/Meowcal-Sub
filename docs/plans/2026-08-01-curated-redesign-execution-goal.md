# Meowcal Sub Curated Redesign — Continuation Goal

Use this file as the execution prompt when continuing the redesign with any
model, including a lower-cost model. It converts the approved product spec into
small, explicit decisions and evidence gates. Do not shorten the objective to
the easiest passing subset.

## Product outcome

A Windows user installs Meowcal Sub, completes one guided setup, selects a
Chinese or Japanese subtitle region, and watches a TV series with private,
low-latency English translation from Tencent HY-MT. The user does not need to
understand Foundry, llama.cpp, GGUF files, endpoints, ports, or command lines.

## Selected visual and frontend direction

- Use the seven approved reference screens under `D:\Downloads\ref` as the
  visual baseline.
- Preserve the calm deep-navy cinematic atmosphere, restrained glass, thin
  luminous borders, and subtle cat identity.
- Improve the references by tightening oversized headings, excess glow,
  repeated cards, empty space, contrast, realistic desktop density, and
  familiar Windows interaction patterns.
- ADR-0003 is approved: keep Tauri/Rust, add a Vite multi-page build, use
  TypeScript controllers and Lit 3 custom elements for Home and setup, and
  preserve the proven overlay/selector paths during the MVP migration.
- Normal mode has one contextual primary action and never exposes Foundry,
  endpoint, port, model-selection, prompt-budget, or raw OCR tuning concepts.

## Canonical sources, in order

1. Live code and Git state in the implementation worktree.
2. `CLAUDE.md` and `docs/AGENT_GUIDE.md`.
3. `docs/plans/2026-07-29-curated-local-translation-app-spec.md`.
4. `docs/plans/2026-07-30-redesign-completion-audit.md`.
5. This continuation goal.

If a historical plan contradicts these sources, do not revive it. The old
Python/OpenSubtitles MeoCoSub2 direction remains out of scope.

## Current delivery mode

- Authoritative implementation worktree:
  `D:\CodexWorktrees\meowcal-main-merge`.
- Preserve unrelated user changes in every other checkout and worktree.
- Continue with small, coherent checkpoint commits.
- The user authorized direct integration for this large MVP redesign. Do not
  create a PR merely to continue this goal. Targeted patches after the MVP may
  return to the normal PR workflow.
- Never close issues or change GitHub repository settings without current,
  requirement-specific evidence.
- A behavior change invalidates earlier manual validation for the behavior it
  affects. Re-run the relevant Windows gate.

## Non-negotiable product contracts

- Keep Tauri 2, Rust, Windows OCR, and the existing overlay approach unless a
  separately reviewed decision replaces a boundary.
- HY-MT is the only supported normal-mode translation engine.
- The app owns download, verification, install, startup, health, repair,
  rollback, upgrade, and shutdown.
- Developer-only model and endpoint controls stay hidden and disabled by
  default.
- Raw OCR is never presented as a successful translation.
- Deterministic output rejection is not retried as a transient failure.
- Bind the managed runtime only to loopback and keep exactly one app-owned
  runtime process.
- Do not claim a performance improvement without comparable before/after
  measurements.
- Do not claim the redesign complete without x64 evidence and a final real
  30-minute episode regression.

## Immediate next stage — finish the selected UI MVP

The user approved the compact cinematic Windows utility direction and the
incremental Lit implementation. Do not restart concept generation.

1. Finish native Tauri visual QA for Home ready/setup/starting/running/repair,
   the four setup steps, Overlay appearance, and Settings at realistic Windows
   sizes and high-DPI scaling.
2. Compare each implementation state with the approved `D:\Downloads\ref`
   source in one same-viewport comparison; fix every visible P0-P2 issue.
3. Preserve the compact Home contract: readiness, language pair, selected area,
   one contextual primary action, and one quiet local-processing line.
4. Keep normal mode free of Foundry, model names, endpoint URLs, ports, runtime
   files, backend toggles, raw OCR tuning, and capture dimensions.
5. Verify first launch, returning launch, restore-without-jump, keyboard focus,
   resize, setup recovery, start/stop, and overlay adjustment in the native app.
6. Run deterministic tests and one centralized repository validation after the
   last visual fix. Do not repeat full suites inside individual implementation
   lanes.
7. Record exact screenshots and iteration evidence in `design-qa.md`.
8. Commit and directly integrate this authorized MVP only after the native gate
   passes; the user's real episode test remains the final product gate.

## Remaining release work after visual selection

Work in this order unless new evidence changes the dependency:

1. Finish the selected UI direction and its focused manual gate.
2. Prove capture -> OCR -> HY-MT -> overlay on the current ARM64 Windows device,
   including explicit source-only and error presentation.
3. Rehearse clean install, adoption, repair, rollback, upgrade, restart,
   tray Quit, uninstall, and sleep/resume.
4. Record real x64 runtime and performance evidence; package generation alone
   is not runtime evidence.
5. Resolve or document the ARM64 warm p95 performance-budget exception.
6. Continue bounded decomposition of `foundry_local.rs`, `manager.rs`,
   `main.js`, `overlay.js`, and `selector.js`, lowering ratchets at every real
   shrink.
7. Reconcile governance files, live GitHub settings, and issue states with the
   finished product.
8. Run the final ARM64/x64 matrix and a real 30-minute TV episode after the last
   behavior change.

## Low-cost-model execution protocol

At the start of every continuation:

1. Read `CLAUDE.md`, this file, and only the reference directly named by the
   next incomplete requirement. Load the approved spec or completion audit only
   when a contract is unclear or evidence must be updated.
2. Run `git status --short --branch` and inspect only the exact target files,
   contracts, and focused tests. Do not replay broad conversation history or
   re-audit the whole repository for a known task.
3. Classify the task before model work: exact one-file/config/CSS work uses the
   weakest execution level; known bugs with a focused test use a small bounded
   worker; unknown root cause, shared interfaces, security, migration,
   concurrency, or deployment require stronger planning or verification.
4. State one bounded outcome, owned files, forbidden files, focused validation,
   and the evidence that proves completion.
5. Add or update a failing test when behavior can be automated, then make the
   smallest coherent implementation that preserves the full product contract.
6. Workers run focused tests only and return a compact changed-files/test/risk
   summary. They do not run installs, full suites, or broad exploration.
7. Use at most one automatic repair. Escalate uncertainty instead of repeating
   worker loops.
8. Run one centralized repository validation after all accepted lanes finish.
9. Manually verify visible Windows behavior and update exact evidence when the
   checkpoint changes user-facing behavior.
10. Commit only scoped files. Push only according to the current delivery mode.

Parallel workers are allowed only for at most two disjoint file sets with
stable contracts and independent focused tests. Shared interfaces, generated
sources, or ordering dependencies must remain sequential. Parallelism optimizes
latency, not total token use.

When something fails:

- Capture the exact error and determine whether it is product code, test
  infrastructure, hardware, or missing external evidence.
- Try safe local alternatives before asking the user.
- Do not edit a baseline, threshold, expected output, or evidence document only
  to make a failure disappear.
- Do not mark an item complete when evidence is indirect, stale, or from the
  wrong architecture.

## Checkpoint report format

Every checkpoint report must contain:

- outcome achieved;
- files and behavior changed;
- tests and manual evidence with pass/fail results;
- current commit and push state;
- remaining release blockers;
- the single next highest-value action.

## Definition of done

The goal is complete only when every definition-of-done row in
`docs/plans/2026-07-30-redesign-completion-audit.md` has current, direct evidence
and no required item remains open. In particular, completion requires the user-
selected UI, a passing guided HY-MT sample, returning launch without command
line steps, correct OCR/translation display semantics, exactly one owned
runtime, ARM64 and x64 measurements, clean lifecycle rehearsals, and a final
30-minute episode regression.
