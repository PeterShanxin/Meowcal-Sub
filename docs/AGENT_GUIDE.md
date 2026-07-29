# Agent Guide

This is the canonical working contract for automated and human-assisted changes
to Meowcal Sub.

## Read order

1. live source, tests, and `.github/workflows/`;
2. this guide and `docs/CODING_STANDARDS.md`;
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
```

The default `All` stage is required before handoff. It runs its own contract
tests, prepares validation resources, and uses the tracked Cargo lockfile. A
failed clean-checkout prerequisite is a repository defect, not a reason to skip
verification.

Browser mode does not prove Tauri-only capture, OCR, selector, overlay, tray,
window, installer, or runtime-process behavior.

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
- `docs/ARCHITECTURE.md`: current architecture after issue #30 lands.
- `docs/adr/`: accepted or proposed cross-cutting decisions.
- `docs/plans/`: dated plans and evidence, not standing policy.
- `docs/archive/`: superseded historical context.
