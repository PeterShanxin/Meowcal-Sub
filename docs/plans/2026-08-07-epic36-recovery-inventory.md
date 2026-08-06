# Epic #36 Recovery Baseline Inventory

**Date**: 2026-08-07 02:00+08
**Author**: recovery automation (issue #79)
**Status**: draft, awaiting external review

## 1. Incident Summary

During the previous Epic #36 implementation attempt, the local repository at
`D:\Repos\Meowcal-Sub` suffered Git object and `refs/` loss. Remote inspection
of `PeterShanxin/Meowcal-Sub` confirmed:

- The claimed #31-#34 implementation was **not present** on GitHub `main`.
- Three local commits (`21b3697`, `34919e2`, `4aa7975`) do not resolve remotely.
- The original monolithic `commands.rs` (2066 lines) and `main.js` (1987 lines)
  remain intact on remote `main`.
- The two refactor branch names (`refactor/extract-lifecycle-31`,
  `refactor/remove-dead-mainjs-33`) do not exist remotely.
- Required verification was not completed in a reproducible manner.

The damaged working tree is treated as **untrusted reference material**.
All future completion evidence must resolve remotely.

## 2. Authoritative Baseline

| Field | Value |
|-------|-------|
| Fresh clone path | `D:\Repos\Meowcal-Sub-epic36-clean` |
| Damaged tree path | `D:\Repos\Meowcal-Sub` |
| Remote URL | `https://github.com/PeterShanxin/Meowcal-Sub.git` |
| Starting `main` SHA | `f8c62d4450eafd408180e4f076b92b15981b2200` |
| Date/time | 2026-08-07 02:08+08 |
| Damaged tree HEAD | `4aa797590e589e899a696c952ba396d1fac0140b` (3 ahead, 196 behind `origin/main`) |
| Windows | Windows 11, build 26200 |
| Architecture | ARM64 (x86_64 emulation via Git Bash) |
| Rust | 1.93.0 |
| Node | 22.22.2 |
| Clean status | Confirmed — fresh clone `git status` shows clean |

### Verification Results

| Stage | Result | Notes |
|-------|--------|-------|
| Contract tests | PASS | verify.ps1 contract tests |
| Engine support tests | PASS | Engine support contract |
| Rust format | PASS | cargo fmt --check |
| Rust clippy | PASS | cargo clippy --locked -- -D warnings |
| Rust unit tests | PASS | 277 passed, 0 failed, 3 ignored; 1.37s |
| Rust IPC integration tests | PASS | 16 passed, 0 failed |
| Frontend install | PASS | npm ci, 169 packages, 0 vulnerabilities |
| Product version sync | PASS | 0.6.7 synchronized across all records |
| Frontend format | PASS | prettier --check |
| Frontend lint | PASS | 10 warnings (at ceiling), 0 errors |
| Frontend typecheck | PASS | tsc --noEmit |
| Frontend build | PASS | vite build, 67 modules |
| Maintainability ratchets | PASS | 134 production files, 17 legacy exceptions |
| Frontend unit tests | PASS | 26 files, 201 tests; coverage above floors |
| Browser bridge smoke | PASS | 4/4 playwright tests |
| Dependency audit | PASS | 0 high-severity vulnerabilities |

Command: `.\scripts\verify.ps1 -Stage All`
Elapsed: ~6 minutes
Exit code: 0

## 3. Difference Inventory

### 3.1 Structural Differences

| Path or change group | Damaged-tree state | Remote `main` state | Classification | Target issue | Decision | Reason |
|---|---|---|---|---|---|---|
| `src-tauri/src/commands.rs` | Deleted (replaced by `commands/` directory) | Exists (2066 lines, monolithic) | Candidate design for #31 | #31 | Reimplement cleanly later | Built on 196-commit-stale base; would revert config durability fixes (#69), 0.6.7 release, update infrastructure |
| `src-tauri/src/commands/*.rs` (8 files) | New (2141 total lines) | Does not exist | Candidate design for #31 | #31 | Retain as design reference only | Decomposed from 196-commit-stale base; structural approach informative but code untrustworthy |
| `src/scripts/main.js` | Deleted | Exists (1987 lines) | Candidate design for #33 | #33 | Reimplement cleanly later | #33 work removed main.js but deleted modules that gained fixes on remote main |
| `src/ui/*.ts` (19 files) | Missing from damaged tree | Exists on remote | Candidate design for #33 | #33 | Already present on remote | The Lit/TS UI was added on remote main independently |
| `src/scripts/overlay.js` | 1022 lines (modified) | 1130 lines | Candidate design for #34 | #34 | Reimplement cleanly later | 108 lines removed; built on stale base with different module boundaries |
| `src/scripts/selector.js` | 774 lines (modified) | 820 lines | Candidate design for #34 | #34 | Reimplement cleanly later | 46 lines removed; built on stale base |
| `src/scripts/overlay-clip-loop.js` | New (damaged only) | Does not exist | Candidate design for #34 | #34 | Retain as design reference only | Extracted from overlay.js; approach informative |
| `src/scripts/selector-dim-overlay.js` | New (damaged only) | Does not exist | Candidate design for #34 | #34 | Retain as design reference only | Extracted from selector.js; approach informative |
| `src-tauri/src/llm/foundry_local.rs` | 1175 lines (modified) | 1719 lines | Candidate design for #32 | #32 | Reimplement cleanly later | 544 lines removed; restructured but based on stale code |
| `src-tauri/src/llm/manager.rs` | 946 lines (modified) | 1234 lines | Candidate design for #32 | #32 | Reimplement cleanly later | 288 lines removed; restructured but based on stale code |
| `src-tauri/src/llm/chat_wire.rs` | New (129 lines) | Does not exist | Candidate design for #32 | #32 | Retain as design reference only | Extracted wire format |
| `src-tauri/src/llm/foundry_cli.rs` | New (335 lines) | Does not exist | Candidate design for #32 | #32 | Retain as design reference only | Extracted CLI config |
| `src-tauri/src/llm/foundry_models.rs` | New (227 lines) | Does not exist | Candidate design for #32 | #32 | Retain as design reference only | Extracted model definitions |
| `src-tauri/src/llm/manager_tests.rs` | New (302 lines) | Does not exist | Candidate design for #32 | #32 | Retain as design reference only | Tests for refactored manager |
| `src-tauri/src/llm/tier.rs` | New (83 lines) | Does not exist | Candidate design for #32 | #32 | Retain as design reference only | Tier abstraction |
| `src-tauri/src/llm/transport_errors.rs` | New (136 lines) | Does not exist | Candidate design for #32 | #32 | Retain as design reference only | Error type extraction |
| `src-tauri/src/engine_launch.rs` | New | Does not exist | Unrelated product behavior change | N/A | Discard | Not in scope for #31-#35; unknown provenance |
| `src-tauri/src/http_config.rs` | New | Does not exist | Unrelated product behavior change | N/A | Discard | Not in scope; unknown provenance |
| `src-tauri/src/http_config_tests.rs` | New | Does not exist | Unrelated product behavior change | N/A | Discard | Not in scope |
| `src-tauri/src/ocr_corruption.rs` | New | Does not exist | Unrelated product behavior change | N/A | Discard | Not in scope |
| `src-tauri/src/ocr_corruption_tests.rs` | New | Does not exist | Unrelated product behavior change | N/A | Discard | Not in scope |
| `src-tauri/src/ocr_recent_lines.rs` | New | Does not exist | Unrelated product behavior change | N/A | Discard | Not in scope |
| `src-tauri/src/ocr_recent_lines_tests.rs` | New | Does not exist | Unrelated product behavior change | N/A | Discard | Not in scope |
| `src-tauri/src/pipeline_deadline.rs` | New | Does not exist | Unrelated product behavior change | N/A | Discard | Not in scope |
| `src-tauri/src/pipeline_notices.rs` | New | Does not exist | Unrelated product behavior change | N/A | Discard | Not in scope |
| `src-tauri/src/pipeline_repeat_policy.rs` | New | Does not exist | Unrelated product behavior change | N/A | Discard | Not in scope |
| `config/maintainability-baseline.json` | Modified (decomposed structure) | Monolithic structure | Candidate design for #31-#34 | #79 | Discard (this PR) | Remote baseline is authoritative; will update per-lane |
| `docs/AGENT_GUIDE.md` | Modified | Current | Stale or conflicts | N/A | Discard | Remote version is authoritative |
| `docs/evidence/2026-08-05-arm64-*.json` (2 files) | New evidence | Does not exist | Already present on remote | N/A | Retain as reference | Evidence from local testing; not production code |
| `src-tauri/resources/OverlayHost*` (100+ WinUI3 DLLs) | Present | Does not exist in clean clone | Generated or machine-specific | N/A | Discard | Build artifacts; should be generated locally |
| `src-tauri/src/config.rs` | 595 lines | 595 lines | Already present on remote | #31 | No change | Same on both trees |
| `src-tauri/src/main.rs` | 519 lines | 519 lines | Already present on remote | #31 | No change | Same on both trees |
| `src-tauri/src/http_server.rs` | 669 lines | 727 lines | Candidate design for #31 | #31 | Reimplement cleanly later | 58 lines removed; modified from stale base |
| Various `.claude/`, `.vscode/`, `.workbuddy/`, `.kilo/`, `.remember/` | Present | Not in fresh clone | Private or unsafe | N/A | Excluded from inventory | IDE/agent config |

### 3.2 Summary Counts

| Classification | Count |
|---|---|
| Already present on remote `main` | 3 groups |
| Candidate design for #31 | 3 groups |
| Candidate design for #32 | 7 groups |
| Candidate design for #33 | 2 groups |
| Candidate design for #34 | 4 groups |
| Unrelated product behavior change | 10 files |
| Stale or conflicts with current `main` | 1 |
| Generated or machine-specific | 1 |
| Private or unsafe to retain | 4 directories |

## 4. Recovery Decisions

### Accepted (documentation/metadata only)

- This recovery inventory document.
- No production code copied from the damaged tree.

### Retain as Design Reference Only

- `commands/` directory structure and decomposition approach (#31)
- LLM module extraction pattern: `chat_wire.rs`, `foundry_cli.rs`, `foundry_models.rs`,
  `manager_tests.rs`, `tier.rs`, `transport_errors.rs` (#32)
- Overlay extraction: `overlay-clip-loop.js`, `selector-dim-overlay.js` (#34)
- Evidence files: `2026-08-05-arm64-abandoned-request-frees-slot.json`,
  `2026-08-05-arm64-engine-thread-sweep.json`

### Reimplement Cleanly Later

- All `commands.rs` decomposition (#31)
- All LLM module extraction (#32)
- All `main.js` removal and Lit/TS consolidation (#33)
- All overlay/selector extraction (#34)
- `http_server.rs` size reduction
- `config/maintainability-baseline.json` ceiling updates

### Discarded

- All 10 new Rust files outside #31-#34 scope (engine_launch, http_config,
  ocr_corruption, ocr_recent_lines, pipeline_deadline, pipeline_notices,
  pipeline_repeat_policy, and their test counterparts)
- Build artifacts in `src-tauri/resources/`
- IDE/agent configuration directories
- Stale documentation (remote versions are authoritative)

### Unresolved Questions

- Were the local commits (`21b3697`, `34919e2`, `4aa7975`) ever pushed to any
  remote or were they always local-only?
- Do the 2 evidence files represent valid findings or test artifacts?
- Should `engine_launch.rs` and the other 9 out-of-scope Rust files be tracked
  in separate issues?

## 5. Child-Issue Plan

### #31 — Decompose commands.rs, lifecycle, config, IPC

- **Current remote hotspot**: `src-tauri/src/commands.rs` (2066 lines, monolithic)
- **Candidate extraction concept**: Split into `commands/` directory with
  per-domain modules (capture_region, foundry_local, ocr_languages, settings,
  system_info, translation, wizard) plus generic lifecycle/window/config services
- **Likely files owned**: `src-tauri/src/commands/*.rs`, `src-tauri/src/lib.rs`,
  `src-tauri/src/main.rs`, `src-tauri/src/config.rs`, `src-tauri/src/http_server.rs`
- **Non-goals**: LLM extraction, frontend changes, overlay/selector changes
- **Required tests**: Contract tests for all command modules, IPC adapter tests
- **Required evidence**: Native Windows ARM64 verification of all commands
- **Damaged-tree code**: Commands directory structure may be consulted as design
  reference; do not copy verbatim (196 commits behind)

### #32 — Engine install/runtime, transport, pipeline

- **Current remote hotspot**: `src-tauri/src/llm/foundry_local.rs` (1719 lines),
  `src-tauri/src/llm/manager.rs` (1234 lines), `src-tauri/src/llm/context.rs` (732 lines)
- **Candidate extraction concept**: Separate into `foundry_cli.rs`,
  `foundry_models.rs`, `chat_wire.rs`, `transport_errors.rs`, `tier.rs`,
  `manager_tests.rs` modules
- **Likely files owned**: `src-tauri/src/llm/*.rs`,
  `src-tauri/src/engine_manifest.rs`, `src-tauri/src/engine_install_transaction.rs`,
  `src-tauri/src/hy_mt_runtime.rs`, `src-tauri/src/pipeline_translation.rs`
- **Non-goals**: New engine backends, build system changes, UI changes
- **Required tests**: Unit tests for each extracted module, integration tests
- **Required evidence**: Native Windows ARM64 subtitle translation validation
- **Damaged-tree code**: LLM module structure may be consulted as design reference

### #33 — Main/setup frontend migration

- **Current remote hotspot**: `src/scripts/main.js` (1987 lines), plus
  `src/ui/*.ts` Lit components
- **Candidate extraction concept**: Remove dead code from `main.js`, consolidate
  remaining initialization into Lit component lifecycle
- **Likely files owned**: `src/scripts/main.js`, `src/ui/*.ts`,
  `src/entries/main.ts`
- **Non-goals**: Backend changes, overlay/selector changes, new features
- **Required tests**: Frontend component tests, browser bridge smoke
- **Required evidence**: Browser mode validation; native Windows UI validation
- **Damaged-tree code**: `main.js` was deleted locally; approach may be referenced
  but the deletion must account for remote-main fixes added after the stale base

### #34 — Overlay/selector migration

- **Current remote hotspot**: `src/scripts/overlay.js` (1130 lines),
  `src/scripts/selector.js` (820 lines)
- **Candidate extraction concept**: Extract `overlay-clip-loop.js` and
  `selector-dim-overlay.js` modules
- **Likely files owned**: `src/scripts/overlay.js`, `src/scripts/selector.js`,
  extracted overlay/selector modules
- **Non-goals**: Backend changes, Lit migration, new overlay features
- **Required tests**: Component tests for extracted modules
- **Required evidence**: Native Windows overlay and selector behavior validation
- **Damaged-tree code**: Extracted modules may be consulted as design reference

## 6. Remote Evidence Policy

Future child issues (#31-#35) **must** provide:

- A pushed branch with remote-resolving commits
- A real GitHub pull request (not a pseudo-review or local-only branch)
- Complete CI results (all verification stages)
- Clean-checkout verification on the PR branch
- Before/after ratchet measurements (`config/maintainability-baseline.json`)
- Fresh named Windows evidence (architecture, build, scenario, result)
- An issue comment linking the PR and valid merge SHA

Claims without remote-resolving evidence are not accepted as completion.
Browser mode does not prove native Windows behavior.

## 7. Remaining Risks

1. **Accidental regression of newer `main` fixes**: The damaged tree is 196 commits
   behind remote `main`. Any copied production code would revert config durability
   fixes (#69), 0.6.7 correctness release patches, in-app update infrastructure,
   and ARM64 compilation safeguards.

2. **Behavior changes hidden inside extraction**: The local refactoring introduced
   10 new Rust source files outside the defined #31-#34 scope. These represent
   unapproved product behavior changes.

3. **Incomplete ARM64 verification**: No evidence exists that the local refactoring
   was tested on native ARM64 Windows with live OCR and translation.

4. **False confidence from formatting/typecheck-only validation**: The damaged
   tree's code may compile but has no deterministic test coverage for the refactored
   paths.

5. **Old documentation contradicting live repository settings**: The damaged
   tree's `config/maintainability-baseline.json` and `docs/AGENT_GUIDE.md` differ
   from remote `main`. Remote versions are authoritative.

6. **Manual validation being skipped again**: The previous attempt claimed
   completion without fresh Windows evidence. The remote evidence policy above
   explicitly prevents this.
