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

## Immediate next stage — visual direction and sensible UI redesign

This stage comes before further broad production-UI implementation.

1. Audit the current first launch, returning launch, select-region, readiness,
   start, stop, repair, and failure paths.
2. Preserve the core flow: select subtitle area, see one private-engine status,
   start watching, stop watching.
3. Generate exactly three independent Windows desktop concepts grounded in the
   current app and its orange/coral identity. The concepts must differ in
   information hierarchy and interaction structure, not only color.
4. Normal mode must not expose Foundry, model names, endpoint URLs, ports,
   runtime files, or backend toggles.
5. Each concept must show realistic states for ready, setup required, starting,
   translating, and repair required without turning the main screen into a
   diagnostics dashboard.
6. Stop after presenting the three concepts. Do not implement the production UI
   until the user selects or combines a direction.
7. Record the selected direction as a visual target and acceptance checklist.
8. Implement it as a separate behavior-visible wave, then run fresh keyboard,
   focus, resize, first-launch, returning-launch, and Windows manual validation.

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

1. Read this file, the approved spec, the completion audit, `CLAUDE.md`, and
   `docs/AGENT_GUIDE.md`.
2. Run `git status --short --branch`, `git log -5 --oneline --decorate`, and
   inspect the exact files named by the next incomplete requirement.
3. State one bounded outcome for the current checkpoint and the evidence that
   will prove it. Do not start an unrelated cleanup.
4. Inspect before editing. Do not infer current behavior from old chat text.
5. Add or update a failing test when the behavior can be automated.
6. Make the smallest coherent implementation that moves the full product goal
   forward; do not substitute a narrower product contract.
7. Run focused tests, then the repository verifier appropriate to the touched
   area.
8. Manually verify visible Windows behavior when required.
9. Update the completion audit with exact evidence and exact missing proof.
10. Commit only scoped files. Push only according to the current delivery mode.

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
