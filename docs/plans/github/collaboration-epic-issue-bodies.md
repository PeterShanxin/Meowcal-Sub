# GitHub Drafts: Collaboration Epic and Child Issues

**Status:** Exact draft; do not post or change settings without approval
**Repository:** `PeterShanxin/Meowcal-Sub`

---

## Collaboration epic

### Title

`Epic: Standardize repository collaboration and change history`

### Labels

`epic`, `type:maintenance`, `area:governance`, `priority:p1`

### Body

```markdown
## Problem

Meowcal Sub has required Rust CI checks, but collaboration and repository history contracts remain informal:

- conventional commits are used but not specified or enforced;
- package version sources disagree;
- PR titles and bodies have no template/contract;
- merge commits, squash, and rebase are all enabled;
- merged branches are not deleted automatically;
- issue intake and labels are unstructured;
- unresolved review conversations do not block merge;
- ownership is implicit;
- architectural decisions and historical plans are not separated consistently.

Make changes easy to understand before review, validate during review, and reconstruct later without adding solo-maintainer bureaucracy.

## Principles

- Keep rules proportional to contributor count.
- Prefer CI-backed contracts over local-only hooks.
- Preserve PR-first delivery and explicit manual Windows gates.
- Keep an admin recovery/bypass path.
- Do not require impossible self-review.
- Activate independent human approval only when an eligible maintainer exists.
- Keep operational guidance, coding rules, ADRs, and historical plans in their correct document classes.
- Apply documentation and live GitHub settings together.

## Child issues

- [ ] #[C1_NUMBER] — Define commit and version contracts
- [ ] #[C2_NUMBER] — Standardize pull request titles and descriptions
- [ ] #[C3_NUMBER] — Make merge history and branch cleanup predictable
- [ ] #[C4_NUMBER] — Add issue forms and a minimal label taxonomy
- [ ] #[C5_NUMBER] — Define ownership and review governance
- [ ] #[C6_NUMBER] — Establish lightweight architecture decision records

## Dependency order

```text
C1 commit/version contract -> C2 PR contract -> C3 merge settings
C4 issue forms -----------------------------------------------┐
C5 ownership/review ------------------------------------------┼-> consistency review
C6 ADRs ------------------------------------------------------┘
```

## Delivery model

- One focused PR per child issue unless a smaller split is justified.
- Repository-setting changes include before/after API or UI evidence.
- Use disposable draft issues/PRs when rendered behavior must be tested.
- Clean up disposable artifacts after validation.
- No external write occurs until its exact payload is approved.

## Definition of done

- commit messages and package versions have one documented enforceable contract;
- PR titles/bodies expose intent, scope, validation, risk, manual gates, and issue linkage;
- merge strategy and generated history match documentation;
- merged branches clean up predictably;
- issue forms collect enough triage evidence without label bureaucracy;
- unresolved review conversations block merge;
- ownership and solo-maintainer activation rules are explicit;
- ADRs have an indexed lightweight lifecycle;
- live settings, CI, contributor docs, agent guidance, and coding standards contain no known contradiction.

## Non-goals

- Product behavior changes.
- Automatic merge.
- Automatic release/versioning.
- Removing admin bypass.
- Mandatory human approval while no independent eligible reviewer exists.
- Rewriting Git history.
- Converting every historical plan into an ADR.
```

---

## C1

### Title

`Define and enforce commit messages and package version ownership`

### Labels

`type:maintenance`, `area:governance`, `priority:p1`

### Milestone

`Wave 1 — Repository foundation`

### Body

```markdown
Parent: #[COLLABORATION_EPIC_NUMBER]

## Problem

Commit messages generally resemble Conventional Commits, but no repository contract exists. Package metadata also disagrees: `package.json` and Tauri config use `0.5.0`, while Cargo uses `0.1.0`.

## Scope

- define allowed commit types, optional scopes, subject rules, breaking changes, and examples;
- define merge/revert/dependency-bot exceptions;
- choose one authoritative package version owner or a verifier that requires exact agreement;
- add CI enforcement reliable on Windows and clean worktrees;
- document release-version changes without implementing release automation;
- ensure local helper hooks are optional feedback, never the only gate.

## Acceptance criteria

- valid and invalid examples are unambiguous;
- CI rejects a deliberate invalid commit/PR title according to the chosen contract;
- Cargo, Tauri, and package versions cannot drift silently;
- merge commits and automated commits have documented handling;
- contributor and agent guidance link to one contract;
- no historical commit rewriting is required.
```

---

## C2

### Title

`Standardize pull request titles and descriptions`

### Labels

`type:maintenance`, `area:governance`, `priority:p1`

### Milestone

`Wave 1 — Repository foundation`

### Body

```markdown
Parent: #[COLLABORATION_EPIC_NUMBER]
Depends on: #[C1_NUMBER]

## Scope

- add one PR template covering:
  - intent/problem;
  - scoped changes;
  - non-goals;
  - automated validation;
  - manual Windows validation;
  - risk and rollback;
  - issue linkage;
  - screenshots/recording for visible changes;
  - before/after evidence for performance claims;
- enforce title contract in CI;
- document draft versus ready-for-review expectations;
- keep generated/maintenance PR handling practical.

## Acceptance criteria

- a disposable draft PR proves template rendering and title checks;
- required sections are concise and meaningful;
- visible changes cannot claim completion without fresh manual-gate status;
- performance claims require comparable evidence;
- issue closing keywords are deliberate;
- contributor docs and CI agree.
```

---

## C3

### Title

`Make merge history and branch cleanup predictable`

### Labels

`type:maintenance`, `area:governance`, `priority:p2`

### Milestone

`Wave 1 — Repository foundation`

### Body

```markdown
Parent: #[COLLABORATION_EPIC_NUMBER]
Depends on: #[C2_NUMBER]

## Current settings

- merge commits: enabled;
- squash merge: enabled;
- rebase merge: enabled;
- auto-merge: disabled;
- delete branch on merge: disabled.

## Proposed decision

- keep merge commits enabled;
- disable squash and rebase;
- keep auto-merge disabled;
- enable automatic deletion of merged branches;
- preserve protected/default branch safety and admin recovery.

## Scope

- document chosen history model and generated commit subject behavior;
- verify PR titles are suitable merge subjects;
- change live repository settings only after approval;
- capture before/after settings;
- test cleanup with a disposable branch/PR;
- never delete protected, default, or active worktree branches.

## Acceptance criteria

- repository settings match documentation;
- one disposable merge proves expected first-parent history;
- merged disposable branch is deleted automatically;
- default/protected branches remain safe;
- no existing history is rewritten;
- test artifacts are cleaned up.
```

---

## C4

### Title

`Add structured issue forms and a minimal label taxonomy`

### Labels

`type:maintenance`, `area:governance`, `priority:p1`

### Milestone

`Wave 1 — Repository foundation`

### Body

```markdown
Parent: #[COLLABORATION_EPIC_NUMBER]

## Scope

Issue forms:

- bug report;
- feature request;
- maintenance/refactor;
- config disabling blank issues only if an escape path remains appropriate.

Forms collect only actionable evidence:

- affected version/commit;
- Windows version and architecture;
- expected/actual behavior;
- reproduction;
- logs/screenshots with privacy warning;
- scope/non-goals for planned work;
- manual validation requirement.

Introduce the approved minimal labels for type, area, priority, epic, and manual gate. Remove or rename only after inspecting current use.

## Acceptance criteria

- forms render correctly in disposable issues;
- privacy warning discourages private screen text/log leakage;
- labels have exact names, colors, and descriptions;
- labels do not duplicate milestones;
- every proposed child issue can be classified without inventing a new label;
- disposable issues are closed and clearly marked as tests.
```

---

## C5

### Title

`Define code ownership and review governance`

### Labels

`type:maintenance`, `area:governance`, `priority:p1`

### Milestone

`Wave 1 — Repository foundation`

### Body

```markdown
Parent: #[COLLABORATION_EPIC_NUMBER]

## Current state

The active ruleset requires `Lint & Format` and `Tests`, is not strict with latest `main`, and allows repository-role bypass. It does not require PR review, resolved conversations, or explicit ownership.

## Scope

- add CODEOWNERS for repository, Rust backend, frontend, workflows, packaging, and ADRs;
- require pull-request delivery and resolved review conversations;
- keep required human approvals at zero while the repository is effectively solo-maintained;
- define activation condition for one independent approval;
- preserve admin/repository-role bypass for recovery;
- decide whether required status checks become strict with latest `main`;
- record settings before and after changes.

## Proposed activation rule

Require one approval only when at least one independent eligible maintainer has write access and CODEOWNER review can be satisfied without self-review.

## Acceptance criteria

- ownership paths match live architecture;
- unresolved conversation blocks a disposable PR merge;
- CI remains required;
- solo maintainer is not forced into impossible self-approval;
- stronger approval activation is documented and testable;
- bypass scope and recovery procedure are explicit;
- disposable artifacts are cleaned up.
```

---

## C6

### Title

`Establish lightweight architecture decision records`

### Labels

`type:docs`, `area:governance`, `priority:p1`

### Milestone

`Wave 1 — Repository foundation`

### Body

```markdown
Parent: #[COLLABORATION_EPIC_NUMBER]

## Scope

- add `docs/adr/README.md` index;
- add lightweight ADR template;
- define statuses: proposed, accepted, superseded, rejected;
- define when an ADR is required;
- define supersession/linking rules;
- distinguish ADRs, normative architecture, implementation plans, and historical plans;
- index ADR-0001 for the curated HY-MT stack;
- classify the Python/OpenSubtitles MeoCoSub2 plan as superseded historical context.

## Acceptance criteria

- a contributor can identify where a new cross-cutting decision belongs;
- ADR-0001 is indexed and linked from architecture/spec;
- superseded decisions remain discoverable without remaining authoritative;
- no requirement converts all old plans into ADRs;
- doc links are checked by the unified verification contract;
- contributor and agent guidance reference the ADR policy.
```
