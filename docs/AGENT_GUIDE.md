# Agent Guide

This is the canonical working contract for automated and human-assisted changes
to Meowcal Sub.

## Read order

1. live source, tests, and `.github/workflows/`;
2. this guide, `docs/CODING_STANDARDS.md`, `docs/ARCHITECTURE.md`, and
   `docs/MAINTAINABILITY_BASELINE.md`;
3. accepted ADRs under `docs/adr/`;
4. `CONTRIBUTING.md`;
5. historical plans only as non-normative context.

If guidance conflicts with live behavior, stop and record the contradiction.
Do not silently choose the more convenient interpretation.

## Product boundary

- Keep Tauri 2, Rust, and Windows OCR.
- Tencent HY-MT is the only supported normal-mode translation engine.
- The app owns engine installation, verification, lifecycle, health, repair,
  rollback, and sample translation.
- Generic endpoints and arbitrary models may exist only in disabled-by-default,
  unsupported developer mode.
- Do not implement the archived Python/OpenSubtitles MeoCoSub2 direction.
- Mine `feat/hymt-foundry` selectively; do not merge it wholesale.

## Workspace safety

- Use an isolated worktree for every implementation lane.
- Inspect `git status`, branch, remote, and relevant diffs before editing.
- Preserve unrelated tracked and untracked changes.
- Stage only scoped files.
- Never use destructive reset or checkout commands to erase unknown changes.
- Identify exact PIDs before stopping repository processes.

## Delivery

- Use small, reviewable changes with a coherent checkpoint commit.
- Keep visible behavior separate from structural refactors.
- Give shared config, event, and state-machine contracts one owner.
- Keep Tauri commands and DOM event handlers thin.
- Do not extend known monoliths when a reviewed boundary exists.
- Update normative docs when their contract changes.

External writes require explicit authority. This includes pushes, pull requests,
issue changes, comments, releases, merges, branch deletion, and repository
settings. Never infer merge permission from permission to open a PR.

## Verification

Run the authoritative gate from the repository root:

```powershell
.\scripts\verify.ps1
```

For focused iteration only:

```powershell
.\scripts\verify.ps1 -Stage Lint
.\scripts\verify.ps1 -Stage Test
.\scripts\verify.ps1 -Stage Frontend
```

The default `All` stage is required before handoff. It runs its own contract
tests, prepares validation resources, uses the tracked Cargo and npm lockfiles,
and includes the real browser-to-Rust bridge smoke. A failed clean-checkout
prerequisite is a repository defect, not a reason to skip verification.

Browser mode does not prove Tauri-only capture, OCR, selector, overlay, tray,
window, installer, or runtime-process behavior.

## Development commands

```powershell
.\dev-browser.cmd                                # browser-only UI iteration
.\dev-tauri.cmd                                  # current ARM64 Tauri dev flow
.\scripts\build-package.ps1 -Architecture auto   # architecture-matched MSI/NSIS
```

`build-package.ps1` applies the verified ARM64 compiler safeguards. Browser mode
does not prove Windows OCR, capture, selector, overlay, tray, installer, or
DPI/window behavior.

## Manual gate

After every visible behavior change, require fresh Windows evidence on the
tested commit. Do not reuse an earlier manual pass after changing behavior.
Report hardware architecture, Windows build, scenario, observed result, and
relevant logs or timing.

Never claim completion while the approved real 30-minute episode validation or
other required manual gates remain outstanding.

## Evidence and claims

- Diagnose from source, logs, tests, and measured behavior.
- Keep deterministic validation rejection separate from transient transport
  failure.
- Never label source OCR as successful translation.
- Never claim performance improvement without comparable before/after data.
- Never claim ARM64 evidence proves x64 behavior.
- Treat model/runtime download metadata as security-sensitive supply-chain data.

## Document classes

- `README.md`: current user/developer entry point.
- `CONTRIBUTING.md`, this guide, and `docs/CODING_STANDARDS.md`: normative.
- `docs/ARCHITECTURE.md`: current and target module ownership.
- `docs/MAINTAINABILITY_BASELINE.md`: enforced ceilings and ratchet procedure.
- `docs/adr/`: accepted or proposed cross-cutting decisions.
- `docs/plans/`: dated plans and evidence, not standing policy.
- `docs/archive/`: superseded historical context.

`CLAUDE.md`, `AGENTS.md`, and `agent.md` are pointers to this guide. Keep them
as pointers; put the actual contract here so every agent reads one source.

## Maintaining this guide

Treat this file as living working memory, not a frozen contract. Maintain it
actively whenever an update would help the next agent:

- When you learn a rule, invariant, or gotcha that would have saved you time,
  add it here in the same change that taught it to you.
- When live behavior contradicts a statement here, fix the statement or record
  the contradiction explicitly. Never leave a known-false line standing.
- When a command, path, gate, or ownership boundary changes, update it here
  before handoff.
- Prefer editing an existing line over appending a near-duplicate. Delete
  guidance that no longer applies rather than accumulating exceptions.
- Keep entries short, imperative, and verifiable. Move dated narrative and
  one-off evidence to `docs/plans/`; keep only standing policy here.

Guide edits are normative and reviewable: state what changed and why in the
change description. Do not weaken a safety, privacy, or evidence rule here to
make a task easier — raise the conflict instead.
