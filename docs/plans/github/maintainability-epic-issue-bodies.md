# GitHub Drafts: Maintainability Epic and Child Issues

**Status:** Exact draft; do not post without approval
**Repository:** `PeterShanxin/Meowcal-Sub`

Replace placeholder tokens after GitHub assigns issue numbers.

---

## Maintainability epic

### Title

`Epic: Raise repository readability, verification, and maintainability`

### Labels

`epic`, `type:maintenance`, `area:repository`, `priority:p0`

### Body

```markdown
## Problem

Meowcal Sub has a working Rust/Tauri core, but contributor and verification foundations do not match its product risk:

- fresh local checks fail unless ignored Tauri resources are manually recreated;
- CI knows this workaround but local documentation does not;
- `package.json` and Tauri config are `0.5.0` while Cargo remains `0.1.0`;
- frontend code has no lint, format, or test gate on `main`;
- README and agent guidance describe removed Offline MT and Phi Silica modules;
- central Rust and JavaScript files combine adapters, state machines, I/O, and presentation;
- there is no canonical architecture, maintainability baseline, file-size ratchet, or honest coverage contract.

Make the repository easy to set up, understand, change, verify, and review without weakening Windows behavior or rewriting the product.

## Baseline

| File | Lines |
|---|---:|
| `src-tauri/src/commands.rs` | 2,899 |
| `src/scripts/main.js` | 2,325 |
| `src-tauri/src/llm/foundry_local.rs` | 1,790 |
| `src-tauri/src/llm/manager.rs` | 1,314 |
| `src/scripts/overlay.js` | 1,293 |
| `src/scripts/selector.js` | 820 |

Current normal gate:

- Rust format;
- Rust clippy;
- 56 Rust unit tests;
- 16 IPC integration tests;
- no frontend tests on `main`.

## Principles

- Preserve behavior during structural waves.
- Give visible changes separate scope and a fresh manual Windows gate.
- Use one root verification contract mirrored by CI.
- Ratchet touched code forward without suppression floods.
- Prefer bounded extractions over rewrites.
- Assign shared-contract ownership before parallel decomposition.
- Treat Tauri-only capture/OCR/overlay checks as explicit manual/E2E gates when browser mode cannot prove them.

## Child issues

- [ ] #[B1_NUMBER] — Establish contributor standards and clean-checkout onboarding
- [ ] #[B2_NUMBER] — Establish one authoritative verification command
- [ ] #[B3_NUMBER] — Add frontend quality and test foundations
- [ ] #[B4_NUMBER] — Record architecture and maintainability ratchets
- [ ] #[B5_NUMBER] — Extract Rust application and command boundaries
- [ ] #[B6_NUMBER] — Extract translation and pipeline boundaries
- [ ] #[B7_NUMBER] — Extract main/setup frontend boundaries
- [ ] #[B8_NUMBER] — Extract overlay/capture frontend boundaries
- [ ] #[B9_NUMBER] — Close coverage and clean-checkout verification

## Dependency order

```text
B1 onboarding -> B2 unified verification -> B3 frontend foundation -> B4 boundaries/baseline
                                                              |
                                                              v
                              B5 Rust app lane ────────────────┐
                              B6 translation lane ────────────┼─> B9 closure
                              B7 main frontend lane ──────────┤
                              B8 overlay frontend lane ───────┘
```

## Parallel ownership

After B4:

- B5 owns Rust app lifecycle, config migration, IPC adapters, and generic command extraction.
- B6 owns engine/translation/pipeline/context contracts.
- B7 owns main window and setup frontend modules.
- B8 owns overlay/selector frontend modules.
- shared event/config contracts have one named owner and are not co-edited casually.

## Definition of done

- normative contributor, coding, agent, architecture, and maintainability documents exist;
- clean-checkout instructions reproduce CI;
- one root verification command matches CI;
- frontend formatting, linting, tests, and browser smoke are enforced;
- version and dependency ownership are explicit;
- coverage reports its real scope and floors are proven to fail;
- tracked hotspot files meet the reviewed ceiling or have explicit shrinking exceptions;
- no known contradiction remains among code, commands, CI, and normative docs;
- final structural changes receive fresh Windows behavior regression.

## Non-goals

- Product feature work.
- Repository-wide rewrite.
- Automatic release/versioning.
- Pretending browser mode validates Tauri-only Windows behavior.
```

---

## B1

### Title

`Establish contributor standards and clean-checkout onboarding`

### Labels

`type:docs`, `area:repository`, `priority:p0`

### Milestone

`Wave 1 — Repository foundation`

### Body

```markdown
Parent: #[MAINTAINABILITY_EPIC_NUMBER]

## Scope

- add `CONTRIBUTING.md`;
- add canonical `docs/AGENT_GUIDE.md`;
- add normative `docs/CODING_STANDARDS.md`;
- document Windows/ARM64/x64 prerequisites and exact toolchain/runtime expectations;
- make local resource setup match CI without undocumented manual files;
- distinguish normative docs from historical plans;
- update README and CLAUDE guidance that references removed backends;
- document isolated-worktree and manual-validation rules;
- establish one version ownership rule covering Cargo, Tauri config, and package metadata.

## Acceptance criteria

- a clean worktree can follow one documented path to run current checks;
- no local command depends on an unexplained ignored resource;
- README, CLAUDE, AGENT_GUIDE, and live modules agree about supported backends;
- version sources agree or derive from one documented owner;
- every contributor entry point links to the canonical guidance;
- historical plans are labeled and cannot override normative docs;
- commands are tested on native PowerShell.

## Validation

- clean-checkout rehearsal on Windows;
- every documented command executed;
- documentation link/consistency review;
- no product behavior changes.
```

---

## B2

### Title

`Establish one authoritative repository verification command`

### Labels

`type:maintenance`, `area:repository`, `priority:p0`

### Milestone

`Wave 1 — Repository foundation`

### Body

```markdown
Parent: #[MAINTAINABILITY_EPIC_NUMBER]
Depends on: #[B1_NUMBER]

## Scope

Create one root command that owns verification ordering and exit behavior:

- Rust format;
- Rust clippy with warnings denied;
- Rust unit tests;
- Rust IPC integration tests;
- frontend format and lint;
- frontend unit tests;
- browser-mode smoke tests;
- documentation checks;
- maintainability/coverage verifiers as later waves add them.

CI must call or exactly mirror the same contract. Local and cloud steps must not drift silently.

## Acceptance criteria

- one documented root command runs all authoritative automated checks;
- CI invokes the same scripts/contracts;
- failure in every stage produces a non-zero result;
- temporary build resources are created and cleaned through one owned helper;
- the command works from a clean checkout in PowerShell;
- an intentional failure proves each stage is wired;
- output identifies skipped manual/Tauri-only gates rather than implying coverage.
```

---

## B3

### Title

`Add frontend formatting, linting, tests, and browser smoke foundations`

### Labels

`type:maintenance`, `area:repository`, `area:ui`, `priority:p0`

### Milestone

`Wave 1 — Repository foundation`

### Body

```markdown
Parent: #[MAINTAINABILITY_EPIC_NUMBER]
Depends on: #[B2_NUMBER]

## Problem

`main.js` and `overlay.js` exceed 1,000 lines, but `main` has no frontend test, lint, or format gate. Browser mode exists but is not exercised by CI.

## Scope

- choose and pin a minimal JavaScript formatting/lint toolchain;
- add Node/frontend version contract and lockfile policy;
- test DOM-independent presentation/state helpers;
- add browser-mode health/settings/readiness smoke coverage;
- document Tauri-only limits;
- integrate all checks into the root verify command and CI;
- establish a warning/debt ratchet instead of mass suppressions.

## Acceptance criteria

- frontend format check fails on deliberate drift;
- lint gate fails on a deliberate new violation;
- unit tests run on `main`;
- browser smoke proves frontend/backend bridge health and a representative settings/readiness flow;
- no test claims to validate real capture/OCR/overlay when browser mode returns 501;
- dependencies are pinned reproducibly;
- all new frontend modules meet the coding standard.
```

---

## B4

### Title

`Record architecture boundaries and maintainability ratchets`

### Labels

`type:docs`, `type:maintenance`, `area:repository`, `priority:p0`

### Milestone

`Wave 1 — Repository foundation`

### Body

```markdown
Parent: #[MAINTAINABILITY_EPIC_NUMBER]
Depends on: #[B3_NUMBER]

## Scope

- add `docs/ARCHITECTURE.md`;
- add `docs/MAINTAINABILITY_BASELINE.md`;
- define backend/frontend module ownership;
- define config/event/version boundaries;
- set a reviewed new-file ceiling and explicit legacy exception list;
- add downward-only line-count/lint/coverage ratchets;
- assign disjoint ownership for B5-B8;
- define how baseline changes are updated in the same PR.

## Acceptance criteria

- architecture reflects live code and approved ADRs;
- every hotspot has target boundary and owner;
- new production files cannot exceed the reviewed ceiling without explicit justification;
- legacy hotspot thresholds cannot increase;
- verifier is proven to fail by temporarily lowering/raising a threshold;
- shared config/event modules have one owner before parallel work;
- no decomposition begins until this issue lands.
```

---

## B5

### Title

`Extract Rust application lifecycle and command boundaries`

### Labels

`type:maintenance`, `area:architecture`, `priority:p1`, `gate:manual-validation`

### Milestone

`Wave 6 — Modular decomposition`

### Body

```markdown
Parent: #[MAINTAINABILITY_EPIC_NUMBER]
Depends on: #[B4_NUMBER]

## Ownership

Own:

- `commands.rs` adapter extraction;
- app lifecycle and window restoration services;
- config loading/migration services;
- generic IPC request/response adapters;
- diagnostics/support-bundle boundaries not owned by engine translation.

Do not own engine/translation internals (B6) or frontend modules (B7/B8).

## Rules

- behavior-preserving extractions only;
- Tauri commands become thin adapters;
- no visible lifecycle fix is smuggled into this structural PR;
- tests move with behavior owners;
- public IPC contract changes require separate review.

## Acceptance criteria

- `commands.rs` falls below its approved ratchet target;
- extracted modules have focused ownership and tests;
- format, clippy, tests, bridge contract, and doc checks pass;
- manual start/stop/settings/tray/capture smoke matches the pre-refactor baseline;
- no unrelated engine or frontend files are co-edited.
```

---

## B6

### Title

`Extract translation engine and subtitle pipeline boundaries`

### Labels

`type:maintenance`, `area:architecture`, `area:translation`, `priority:p1`, `gate:manual-validation`

### Milestone

`Wave 6 — Modular decomposition`

### Body

```markdown
Parent: #[MAINTAINABILITY_EPIC_NUMBER]
Depends on: #[B4_NUMBER] and the shared product contracts from the correctness/engine waves

## Ownership

Own:

- engine manifest/install/runtime service boundaries;
- translation transport;
- prompt and response parsing;
- validation and retry classification;
- context;
- session pipeline orchestration;
- translation diagnostics and display-state payloads.

Do not own generic Tauri lifecycle (B5) or frontend presentation (B7/B8).

## Acceptance criteria

- transport, validation, retry, engine state, and pipeline orchestration have separate owners;
- `foundry_local.rs` and `manager.rs` fall below approved ratchet targets;
- old Foundry-specific naming is removed from normal product contracts;
- tests preserve approved product behavior and failure states;
- no private screen text enters production logs by default;
- manual translation regression is repeated after final structural change.
```

---

## B7

### Title

`Extract main window and setup frontend boundaries`

### Labels

`type:maintenance`, `area:architecture`, `area:ui`, `priority:p1`, `gate:manual-validation`

### Milestone

`Wave 6 — Modular decomposition`

### Body

```markdown
Parent: #[MAINTAINABILITY_EPIC_NUMBER]
Depends on: #[B4_NUMBER] and stable engine/setup contracts

## Ownership

Own:

- main-window bootstrap;
- setup flow presentation;
- engine state/repair presentation;
- OCR/language readiness presentation;
- session controls;
- settings persistence adapters;
- developer diagnostics presentation.

Do not own backend state machines or overlay/selector code.

## Acceptance criteria

- `main.js` falls below its approved ratchet target;
- DOM handlers are thin and testable state/presentation helpers are isolated;
- no duplicate readiness owner remains;
- browser unit/smoke checks cover supported paths;
- Tauri manual setup/start/stop/settings regression passes;
- visible changes, if any, are split into a separate scope and manual gate.
```

---

## B8

### Title

`Extract overlay and capture-selection frontend boundaries`

### Labels

`type:maintenance`, `area:architecture`, `area:ui`, `priority:p1`, `gate:manual-validation`

### Milestone

`Wave 6 — Modular decomposition`

### Body

```markdown
Parent: #[MAINTAINABILITY_EPIC_NUMBER]
Depends on: #[B4_NUMBER] and stable pipeline/display payloads

## Ownership

Own:

- overlay session/presentation state;
- subtitle geometry and clipping;
- drag/resize interactions;
- selector state and geometry;
- bridge/event adapters for overlay and selector.

Do not own backend capture, translation, or main/setup modules.

## Acceptance criteria

- `overlay.js` and `selector.js` fall below approved ratchet targets;
- geometry/state helpers have deterministic tests;
- event listeners and timers have explicit cleanup ownership;
- mixed-DPI, drag, resize, click-through, clipping, and region persistence are manually regressed;
- visible behavior remains unchanged unless separately scoped.
```

---

## B9

### Title

`Close risk-based coverage and clean-checkout verification`

### Labels

`type:maintenance`, `area:repository`, `priority:p0`, `gate:manual-validation`

### Milestone

`Wave 7 — Release closure`

### Body

```markdown
Parent: #[MAINTAINABILITY_EPIC_NUMBER]
Depends on: #[B5_NUMBER], #[B6_NUMBER], #[B7_NUMBER], #[B8_NUMBER]

## Scope

- define named risk-based coverage areas and exact measured floors;
- prove instrumentation includes intended Rust and frontend modules;
- prove every threshold fails when crossed;
- rehearse setup and full verification from a clean checkout;
- verify CI/local parity;
- update maintainability baseline after decomposition;
- run final Windows behavior regression for structural work;
- review normative docs, commands, CI, and code for contradictions.

## Acceptance criteria

- clean checkout reaches the full automated gate with documented prerequisites only;
- every verification stage and coverage floor is proven capable of failing;
- coverage claims name their exact scope;
- no tracked production hotspot exceeds the approved ceiling, or every exception is explicit and lower than baseline;
- all normative links resolve;
- final manual Windows regression passes;
- parent epic closes with before/after measurements.
```
