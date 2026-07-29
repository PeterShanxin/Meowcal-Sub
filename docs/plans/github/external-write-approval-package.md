# GitHub External-Write Approval Package

**Prepared:** 2026-07-29
**Repository:** `PeterShanxin/Meowcal-Sub`
**Status:** Draft only. No item, comment, label, milestone, PR, push, merge, or setting has been changed.

## 1. Exact creation order

Create items in this order so placeholder tokens can be resolved deterministically:

1. labels;
2. milestones;
3. Product child issues A2-A11;
4. Product epic, replacing `[A*_NUMBER]`;
5. add the approved comment to existing Issue #15, replacing `[PRODUCT_EPIC_NUMBER]`;
6. update Product child bodies with `[PRODUCT_EPIC_NUMBER]` and dependency numbers;
7. Maintainability child issues B1-B9;
8. Maintainability epic, replacing `[B*_NUMBER]`;
9. update B child bodies with parent/dependency numbers;
10. Collaboration child issues C1-C6;
11. Collaboration epic, replacing `[C*_NUMBER]`;
12. update C child bodies with parent/dependency numbers;
13. no repository setting changes until their file PRs and disposable tests are ready.

Exact issue bodies:

- `docs/plans/github/product-epic-issue-bodies.md`
- `docs/plans/github/maintainability-epic-issue-bodies.md`
- `docs/plans/github/collaboration-epic-issue-bodies.md`

## 2. Labels

Create only missing labels. If a same-purpose label exists, propose a rename/migration before changing it.

| Name | Color | Description |
|---|---|---|
| `epic` | `5319E7` | Parent issue coordinating multiple bounded child issues |
| `type:bug` | `D73A4A` | Existing behavior is incorrect |
| `type:feature` | `0E8A16` | New user-facing or system capability |
| `type:maintenance` | `6A737D` | Refactor, test, tooling, dependency, or operational upkeep |
| `type:docs` | `0075CA` | Documentation-only or documentation-governance work |
| `area:engine` | `1D76DB` | HY-MT model, install, manifest, runtime, or process lifecycle |
| `area:ocr` | `C5DEF5` | Windows OCR, language capability, preprocessing, or recognition |
| `area:translation` | `BFDADC` | Prompt, model request, response parsing, validation, or quality |
| `area:pipeline` | `FBCA04` | Capture-to-overlay orchestration, retry, dedupe, queue, or state |
| `area:ui` | `D4C5F9` | Main, setup, selector, or overlay frontend behavior |
| `area:lifecycle` | `F9D0C4` | Startup, shutdown, window, tray, sleep, resume, or persistence |
| `area:architecture` | `0052CC` | Module boundaries and structural ownership |
| `area:repository` | `B60205` | CI, tooling, testing, contributor workflow, or maintainability |
| `area:governance` | `7057FF` | GitHub metadata, review, ownership, history, or ADR policy |
| `area:release` | `006B75` | Packaging, upgrade, manual release gate, or release evidence |
| `priority:p0` | `B60205` | Blocks the approved primary user outcome or release gate |
| `priority:p1` | `D93F0B` | Important next-wave work with material risk or leverage |
| `priority:p2` | `FBCA04` | Valuable follow-up not on the immediate critical path |
| `gate:manual-validation` | `E99695` | Cannot close without named fresh manual Windows evidence |

Labels intentionally do not encode wave/status. Milestones own sequencing; checklists own completion.

## 3. Milestones

No due dates until an actual release schedule exists.

| Title | Description |
|---|---|
| `Wave 1 — Repository foundation` | Contributor contracts, unified verification, frontend quality foundation, maintainability baseline, and collaboration metadata |
| `Wave 2 — Critical correctness` | OCR aliases, subtitle validation, retry/fallback state, and curated evaluation fixtures |
| `Wave 3 — Curated engine` | Versioned manifest, install/adoption/repair/rollback, and exact runtime ownership |
| `Wave 4 — Product UX` | One normal-mode local translation setup and separate developer diagnostics |
| `Wave 5 — Lifecycle and performance` | Window restoration, instrumentation, cancellation, dedupe, queue control, and measured budgets |
| `Wave 6 — Modular decomposition` | Behavior-preserving extraction of Rust, translation, main/setup, and overlay/selector lanes |
| `Wave 7 — Release closure` | Clean-checkout, coverage, packaging, ARM64/x64, upgrade/repair, and real episode evidence |

## 4. Existing Issue #15

Approved action:

- add the exact comment in `product-epic-issue-bodies.md`;
- apply labels `type:bug`, `area:ocr`, `priority:p0`, `gate:manual-validation`;
- assign milestone `Wave 2 — Critical correctness`;
- keep issue open;
- do not replace its existing root-cause body.

## 5. Repository settings

### Current snapshot

| Setting | Current |
|---|---|
| Default branch | `main` |
| Merge commits | enabled |
| Squash merge | enabled |
| Rebase merge | enabled |
| Auto-merge | disabled |
| Delete branch on merge | disabled |
| Required checks | `Lint & Format`, `Tests` |
| Strict latest-main checks | disabled |
| Required PR | not configured in current ruleset |
| Required approvals | not configured |
| Required resolved conversations | not configured |
| Repository-role bypass | enabled |

### Proposed final snapshot

Apply through Collaboration issues after their repository-file PRs land and disposable behavior tests are ready:

| Setting | Proposed |
|---|---|
| Default branch | keep `main` |
| Merge commits | keep enabled |
| Squash merge | disable |
| Rebase merge | disable |
| Auto-merge | keep disabled |
| Delete branch on merge | enable |
| Required checks | replace with names produced by the unified verify CI |
| Strict latest-main checks | enable after proving reasonable queue behavior |
| Required PR | enable |
| Required approvals | `0` while solo-maintained |
| Required resolved conversations | enable |
| CODEOWNER approval | do not require while solo-maintained |
| Repository-role bypass | keep for recovery |

Activation rule for one approval/CODEOWNER review:

> Enable when at least one independent eligible maintainer has write access and the rule can be satisfied without self-review.

### Settings that require a second explicit approval

- disabling squash/rebase;
- enabling automatic branch deletion;
- modifying the active ruleset;
- changing required check names;
- enabling strict latest-main enforcement;
- enabling required PR/resolved conversations.

## 6. Branch and PR reconciliation

### PR 1: package/version reconciliation

#### Local source

Commit `0d35889` is currently local `main`, one commit ahead of `origin/main`.

Before push, create a dedicated branch at this commit and add the missing Cargo version alignment:

`build/v0.5-package-config`

#### Proposed title

`build: align v0.5 package metadata and Windows installer targets`

#### Proposed body

```markdown
## Intent

Reconcile the local v0.5 packaging commit before the curated translation redesign branches depend on it.

## Changes

- align `package.json`, Tauri config, and Cargo package version at `0.5.0`;
- target MSI and NSIS Windows bundles;
- configure the current-user English NSIS installer;
- document which file owns future version changes or add a verifier preventing drift.

## Non-goals

- Translation behavior.
- HY-MT integration.
- Release automation.
- Publishing a release.

## Validation

- `cargo fmt --check`
- `cargo clippy -- -D warnings`
- `cargo test --lib`
- `cargo test --test integration_ipc`
- Tauri config/schema validation
- Windows package build for the intended targets

## Risk

Packaging metadata can produce incorrect installer artifacts or version display. No remote release is created by this PR.

## Manual gate

- install generated NSIS package as current user;
- launch and uninstall;
- confirm displayed/application package version;
- confirm no existing user config is removed.
```

#### Approval requested

- create local branch pointer;
- add scoped Cargo/version consistency change;
- push branch;
- open PR;
- do not merge without separate approval.

### PR 2: Wave 0 product decision and baseline

#### Branch

`docs/curated-redesign-wave0`

#### Current commit

`7e3399d`

#### Proposed title

`docs: establish curated local translation redesign baseline`

#### Proposed body

```markdown
## Intent

Record the approved Meowcal Sub product direction and the evidence baseline that governs the redesign before product implementation begins.

## Changes

- approve the curated HY-MT local translation product specification;
- record ADR-0001;
- record repository, correctness, verification, runtime, performance, and GitHub baselines;
- propose Product, Maintainability, and Collaboration epics;
- add exact GitHub external-write drafts.

## Product decision

Meowcal Sub remains Tauri/Rust with Windows OCR. Tencent HY-MT becomes the sole supported normal-mode translation engine. Generic endpoints move to unsupported developer mode. The Python/OpenSubtitles rewrite is superseded for this epic.

## Validation

Current `main` after reproducing CI resource setup:

- `cargo fmt --check`
- `cargo clippy -- -D warnings`
- `cargo test --lib` — 56 passed
- `cargo test --test integration_ipc` — 16 passed

HY-MT prototype branch:

- frontend tests — 3 passed
- Rust unit tests — 62 passed, 1 ignored live test
- integration tests — 16 passed
- format and clippy passed

ARM64 direct HY-MT endpoint baseline:

- p50 487 ms
- p95 671 ms
- 10 representative Chinese subtitle translations

## Risk

Documentation only. It intentionally exposes gaps and does not claim whole-app performance or product completion.

## External state

This PR does not create issues, change repository settings, merge the HY-MT branch, or close Issue #15.
```

#### Dependency

Base this PR on remote `main` only after PR 1 lands, or explicitly retarget/rebase it so the version commit is not hidden inside the documentation PR.

#### Approval requested

- push branch;
- open draft PR;
- do not merge without separate approval.

## 7. Old MeoCoSub2 document

Current file exists only as an untracked file in the primary worktree:

`docs/plans/2026-03-04-meocosub2-design.md`

Recommended approved action:

1. preserve its content;
2. change status from `Approved` to `Superseded`;
3. add `Superseded by: ADR-0001 and the 2026-07-29 curated local translation spec`;
4. move to `docs/archive/plans/2026-03-04-meocosub2-design.md` only after checking for external links;
5. include this as documentation-only scope, not product implementation.

Do not delete the file.

## 8. `feat/hymt-foundry`

Recommended approved action:

- keep the branch/worktree until engine extraction finishes;
- do not push/open one full PR;
- reference commits `301e1b6` and `1f49771` in A5-A8 implementation notes;
- extract bounded code/tests through new branches after shared contracts land;
- delete the branch/worktree only after every retained element is traceably migrated or rejected.

## 9. Exact external authorization choices

The user may approve separately:

1. **Backlog writes** — create labels, milestones, epics, child issues, and update #15.
2. **PR 1 writes** — push package/version branch and open PR 1.
3. **PR 2 writes** — push Wave 0 branch and open draft PR 2 after base reconciliation.
4. **Historical-plan edit** — mark/move MeoCoSub2 as superseded.
5. **Repository settings** — deferred until C3/C5 files and disposable tests are ready.
6. **Wave 1 local implementation** — begin repository foundation in an isolated worktree.

No option implies merge permission.
