# Epic #36 Recovery Baseline Inventory

**Date**: 2026-08-07 02:00+08
**Author**: recovery automation (issue #79)
**Status**: revision 2, awaiting external re-review
**PR**: #80 (draft)

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

**Local-only evidence from the damaged tree is not authoritative completion
evidence and cannot satisfy a manual or automated gate unless independently
reproduced from a clean remote branch.**

## 2. Authoritative Baseline

| Field | Value |
|-------|-------|
| Fresh clone path | `D:\Repos\Meowcal-Sub-epic36-clean` |
| Damaged tree path | `D:\Repos\Meowcal-Sub` (kept read-only) |
| Remote URL | `https://github.com/PeterShanxin/Meowcal-Sub.git` |
| Starting `main` SHA | `f8c62d4450eafd408180e4f076b92b15981b2200` |
| Date/time | 2026-08-07 02:08+08 |
| Damaged tree HEAD | `4aa797590e589e899a696c952ba396d1fac0140b` (3 ahead, 196 behind `origin/main`) |
| Windows | Windows 11, build 26200 |
| Architecture | ARM64 (x86_64 emulation via Git Bash) |
| Rust | 1.93.0 |
| Node | 22.22.2 |
| Clean status | Confirmed — fresh clone `git status` shows clean |

## 3. Verification Evidence

### 3.1 Clean-clone local baseline (commit `f8c62d4`)

Command: `.\scripts\verify.ps1 -Stage All`
Elapsed: ~6 minutes
Exit code: 0

| Stage | Result | Notes |
|-------|--------|-------|
| Contract tests | PASS | verify.ps1 contract tests |
| Engine support tests | PASS | Engine support contract |
| Rust format | PASS | cargo fmt --check |
| Rust clippy | PASS | cargo clippy --locked -- -D warnings |
| Rust unit tests | PASS | 277 passed, 0 failed, 3 ignored |
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

### 3.2 Remote CI status for head `b8bd48f` (before revision)

GitHub reported **no status checks and no workflow runs** for PR #80 head
`b8bd48f08e52fb62c7b5cd2d086be7f40024e793`.

Two facts explain this, and both are now evidenced:

1. **Draft-state suppression.** PR #80 was created as a **draft**. GitHub does
   not dispatch `pull_request` workflow runs while a pull request is in draft
   state; the `ready_for_review` activity type exists in the event's type list
   precisely because marking a draft ready is what releases the run. The
   repository's CI workflow (`.github/workflows/test.yml`) triggers on
   `pull_request` with the default activity types (`opened`, `synchronize`,
   `reopened`) and on `push` to `main`. It has no path filter, so a docs-only
   change would normally still run it. Every other recent `pull_request`-event
   CI run in this repository came from a non-draft branch
   (`fix/config-durability`, `docs/architecture-ratchets-30`, etc.); none came
   from the draft branch `recovery/epic36-remote-baseline-79`.

2. **GitHub Actions global incident (2026-08-06).** After the PR was marked
   ready for review and a `synchronize` push was made (head
   `01fdf94dda866496d658820808c0dd7ae4e206f7`), no workflow run was still
   dispatched. GitHub Status reported an ongoing incident affecting Actions
   starting 2026-08-06 15:22 UTC
   (<https://www.githubstatus.com/incidents/qcvjkzcs7j74>): "Workflow runs are
   still failing, and jobs may remain queued for an extended period before
   starting or may time out"; "Webhook deliveries may be delayed." The
   `pull_request` webhook that would normally create the CI run was therefore
   not delivered/processed during the window of this revision. The repository's
   Actions permission is enabled (`allowed_actions: all`, `enabled: true`) and
   the CI workflow is `active`; there is no repository-side reason the run
   would not be created once GitHub recovers.

Expected resolution: when the GitHub Actions incident clears, the `pull_request`
webhook for head `01fdf94dda866496d658820808c0dd7ae4e206f7` will be delivered
and the CI workflow will create its three jobs (Lint & Format, Tests,
Frontend & Browser). Results will be recorded in this document and the PR body
after they complete. The PR must not be merged before those checks pass.

### 3.3 Verification table

| Evidence | Environment | Commit | Result |
|---|---|---|---|
| Clean-main local baseline | Windows 11 ARM64 (26200), Rust 1.93, Node 22 | `f8c62d4450...` | Pass (all 16 stages) |
| Revised-branch local verification | Windows 11 ARM64 (26200), Rust 1.93, Node 22 | `01fdf94dda866496d658820808c0dd7ae4e206f7` | Pass (277 Rust unit, 16 IPC, 201 frontend, 4 browser smoke, 0 vulnerabilities) |
| GitHub PR checks | GitHub Actions `windows-latest` | `01fdf94dda...` | Not run — GitHub Actions global incident (2026-08-06 15:22 UTC, ongoing); draft state also suppressed earlier runs |

Local `verify.ps1` results are **not** remote CI. They are a clean-checkout
baseline and a docs-only-delta confirmation; only GitHub-hosted checks count as
remote CI.

## 4. Content and Provenance Audit — the ten blanket-discarded Rust files

All ten files are **tracked**, introduced **only** by the local-only consolidated
commit `21b3697` (2026-08-07, local), and do **not** exist on remote `main`.
Their working-tree state is clean (no uncommitted edits). None of the ten is a
pure extraction in the sense of duplicating remote `main` code except where
noted; several are behavior-changing fixes for OPEN product issues #59/#60/#71.

| File/group | Damaged-tree Git status | Provenance | Semantic purpose | Behavior impact | Remote-main equivalent | Classification | Target | Decision | Reason |
|---|---|---|---|---|---|---|---|---|---|
| `src-tauri/src/engine_launch.rs` (206) | Tracked | `21b3697` only | Engine worker-thread policy: `worker_threads` (cores−4, floor 4), `launch_args` appends `--threads` on ARM64 only, honours a manifest-pinned thread count | **Changes behavior** on ARM64 (adds `--threads`); tied to evidence `2026-08-05-arm64-engine-thread-sweep.json` | None on remote `main` (no `--threads`/thread-count logic anywhere) | Candidate #32 engine-runtime boundary + behavior change | #32 or separate #60 follow-up | Reimplement cleanly later; do not copy | New module on 196-commit-stale base; behavior change needs its own ARM64 measurement and review |
| `src-tauri/src/http_config.rs` (143) | Tracked | `21b3697` only | Dev-mode (browser HTTP) config load/save with the #64/#71 durability fix, reusing `config_store`/`config_save`; 500 on refused save | **Changes behavior**: fixes the #71 bug (dev-mode truncated-config reset, app-owned fields blanked) | Remote `http_server.rs` has inline non-durable `load/save_standalone_config` (lines 73-103) | Separate product issue | #71 (OPEN) | File separate follow-up; reimplement cleanly | Fixes an OPEN correctness issue; belongs to its own PR with browser-mode verification |
| `src-tauri/src/http_config_tests.rs` (279) | Tracked | `21b3697` only | Tests for `http_config` (recovery, provenance, app-owned preservation) | Test-only | None | Separate product issue | #71 (OPEN) | File separate follow-up | Test pair of `http_config.rs` |
| `src-tauri/src/ocr_corruption.rs` (201) | Tracked | `21b3697` only | OCR-noise scoring: `corruption_share`, `is_worse_read`, `is_mostly_noise`; comparative rather than absolute; CJK-safe | **Changes behavior**: refuses mangled re-reads (display quality) | None (grep finds no corruption scoring) | Separate product issue | #59 (OPEN) | File separate follow-up; reimplement cleanly | Fixes OPEN issue #59; needs fresh OCR session evidence |
| `src-tauri/src/ocr_corruption_tests.rs` (178) | Tracked | `21b3697` only | Tests pinning noise rules against recorded session text | Test-only | None | Separate product issue | #59 (OPEN) | File separate follow-up | Test pair of `ocr_corruption.rs` |
| `src-tauri/src/ocr_recent_lines.rs` (164) | Tracked | `21b3697` only | `RecentLines`: last-4-lines, 6s window repeat classification (two-line cue fix) | **Changes behavior**: suppresses repeated two-line cues | None (remote `ocr_stability.rs` is single-line only) | Separate product issue | #59 (OPEN) | File separate follow-up; reimplement cleanly | Fixes OPEN issue #59; needs fresh OCR session evidence |
| `src-tauri/src/ocr_recent_lines_tests.rs` (181) | Tracked | `21b3697` only | Tests for `RecentLines` | Test-only | None | Separate product issue | #59 (OPEN) | File separate follow-up | Test pair of `ocr_recent_lines.rs` |
| `src-tauri/src/pipeline_deadline.rs` (188) | Tracked | `21b3697` only | Translation slot deadline (5s), `await_within_deadline`, `SlotOutcome`, fallback reserve | **Changes behavior**: bounds engine call, abandons overdue lines | None (only pacing comments inline in remote `commands.rs`) | Separate product issue | #60 (OPEN) | File separate follow-up; reimplement cleanly | Fixes OPEN issue #60; evidence `2026-08-05-arm64-abandoned-request-frees-slot.json` is local-only |
| `src-tauri/src/pipeline_notices.rs` (164) | Tracked | `21b3697` only | `Notices`: dedup of empty/unreadable notices (emit once on change) | **None (extraction)** | **Inline in remote `commands.rs`** (lines ~1612-1650 use `EMPTY_OCR_CLEAR_FRAMES`, `source_unreadable`) | #32 design reference | #32 | Retain as design reference; reimplement inside #32 | Pure extraction of remote-main inline logic with added tests |
| `src-tauri/src/pipeline_repeat_policy.rs` (92) | Tracked | `21b3697` only | `decide()`: skip duplicate line vs retry passthrough with 2.5s cooldown | **None (extraction)** | **Inline in remote `commands.rs`** (`MOCK_RETRY_COOLDOWN_MS`, `duplicate_line`) | #32 design reference | #32 | Retain as design reference; reimplement inside #32 | Pure extraction of remote-main inline logic with added tests |

### 4.1 Disposition summary (ten files)

- **#32 design reference (extraction-only, inline logic exists on remote `main`)**:
  `pipeline_notices.rs`, `pipeline_repeat_policy.rs` — 2 files.
- **Separate follow-up, behavior-changing fixes for OPEN product issues**:
  `engine_launch.rs` (#60), `pipeline_deadline.rs` (#60),
  `ocr_corruption.rs` + tests (#59), `ocr_recent_lines.rs` + tests (#59),
  `http_config.rs` + tests (#71) — 8 files in 5 groups.
- **Discarded**: none. Every file maps to a live scope or an OPEN issue.
- **Copied into this PR**: none.

The previous "unrelated product behavior change / unknown provenance" label was
insufficient and is withdrawn.

## 5. Local Evidence File Audit

The two local-only evidence files were inspected (metadata, SHA-256, content).
Neither is added to PR #80. Both are **unverified local reference material**.

### 5.1 `docs/evidence/2026-08-05-arm64-abandoned-request-frees-slot.json`

| Field | Value |
|---|---|
| Size | 3,448 bytes |
| Last write | 2026-08-05 17:35+08 |
| SHA-256 | `387f970daa5dcf01505e260e37bd162baca8a9d5af28999a7a77043d5aa9dc09` |
| Content | llama-server slot-release experiment: close client mid-generation, measure cancellation and slot release (9ms), next task start (5ms), 267ms completion |
| Tested commit | `8bc8e1a577fb86cc2d79936c74fccd8b28a15e9e` — **does not resolve on remote** |
| Privacy | No subtitle text, paths, tokens, or machine identifiers; model/runtime names and port only |
| Schema | Understandable (schemaVersion, method, results, finding, decision, limitations) |
| Correspondence | Describes `pipeline_deadline` behavior; that module is **not** on remote `main` |
| Reproducibility | Not independently reproduced from a clean remote branch |
| Relation | Issue #60 |
| Disposition | Retain only as unverified local reference; summarize into the #60 follow-up |

### 5.2 `docs/evidence/2026-08-05-arm64-engine-thread-sweep.json`

| Field | Value |
|---|---|
| Size | 3,824 bytes |
| Last write | 2026-08-05 17:35+08 |
| SHA-256 | `990a406d3f6205561cbe93f98696ea14fa46e8a4777476d6baf01e198506b0b3` |
| Content | Worker-thread sweep (default/4/6/8/12 threads) on Snapdragon X Elite, idle vs loaded; 8 chosen; counter-finding that thread caps do not fix the tail |
| Tested commit | `98353c8` — **resolves** (remote `main` history, "Merge the 0.6.7 correctness release") |
| Privacy | Host model name (X1E80100) and RAM; no subtitle text, paths, tokens, or credentials |
| Schema | Understandable (schemaVersion, method, results, finding, counterFinding, decision, limitations) |
| Correspondence | Describes `engine_launch` thread policy; that module is **not** on remote `main` |
| Reproducibility | Not independently reproduced from a clean remote branch |
| Relation | Issue #60 |
| Disposition | Retain only as unverified local reference; summarize into the #60 follow-up |

Even though the thread-sweep file names a commit that resolves, the module it
describes was never merged, so the file is not completion evidence.

## 6. Difference Inventory

### 6.1 Table

| Path or change group | Damaged-tree state | Remote `main` state | Classification | Target issue | Decision | Reason |
|---|---|---|---|---|---|---|
| `src-tauri/src/commands.rs` | Deleted (replaced by `commands/` directory) | Exists (2066 lines, monolithic) | Candidate design for #31 | #31 | Reimplement cleanly later | Built on 196-commit-stale base; would revert config durability fixes (#69), 0.6.7 release, update infrastructure |
| `src-tauri/src/commands/*.rs` (8 files) | New (2141 total lines) | Does not exist | Candidate design for #31 | #31 | Retain as design reference only | Decomposed from stale base; structural approach informative, code untrustworthy |
| `src/scripts/main.js` | Deleted | Exists (1987 lines) | Candidate design for #33 | #33 | Reimplement cleanly later | #33 work removed main.js but deleted modules gained fixes on remote main |
| `src/ui/*.ts` (19 files) | Absent from damaged tree | Exists on remote | **Already present on remote `main`** | N/A | No change | Lit/TS UI was added on remote main independently; nothing to recover |
| `src/scripts/overlay.js` | 1022 lines (modified) | 1130 lines | Candidate design for #34 | #34 | Reimplement cleanly later | 108 lines removed; stale base with different module boundaries |
| `src/scripts/selector.js` | 774 lines (modified) | 820 lines | Candidate design for #34 | #34 | Reimplement cleanly later | 46 lines removed; stale base |
| `src/scripts/overlay-clip-loop.js` | New (damaged only) | Does not exist | Candidate design for #34 | #34 | Retain as design reference only | Extracted from overlay.js; approach informative |
| `src/scripts/selector-dim-overlay.js` | New (damaged only) | Does not exist | Candidate design for #34 | #34 | Retain as design reference only | Extracted from selector.js; approach informative |
| `src-tauri/src/llm/foundry_local.rs` | 1175 lines (modified) | 1719 lines | Candidate design for #32 | #32 | Reimplement cleanly later | 544 lines removed; restructured on stale base |
| `src-tauri/src/llm/manager.rs` | 946 lines (modified) | 1234 lines | Candidate design for #32 | #32 | Reimplement cleanly later | 288 lines removed; restructured on stale base |
| `src-tauri/src/llm/chat_wire.rs` | New (129) | Does not exist | Candidate design for #32 | #32 | Retain as design reference only | Extracted wire format |
| `src-tauri/src/llm/foundry_cli.rs` | New (335) | Does not exist | Candidate design for #32 | #32 | Retain as design reference only | Extracted CLI config |
| `src-tauri/src/llm/foundry_models.rs` | New (227) | Does not exist | Candidate design for #32 | #32 | Retain as design reference only | Extracted model definitions |
| `src-tauri/src/llm/manager_tests.rs` | New (302) | Does not exist | Candidate design for #32 | #32 | Retain as design reference only | Tests for refactored manager |
| `src-tauri/src/llm/tier.rs` | New (83) | Does not exist | Candidate design for #32 | #32 | Retain as design reference only | Tier abstraction |
| `src-tauri/src/llm/transport_errors.rs` | New (136) | Does not exist | Candidate design for #32 | #32 | Retain as design reference only | Error type extraction |
| `src-tauri/src/pipeline_notices.rs` | New (164) | Inline equivalent in `commands.rs` | #32 design reference | #32 | Retain as design reference only | Pure extraction; see §4 |
| `src-tauri/src/pipeline_repeat_policy.rs` | New (92) | Inline equivalent in `commands.rs` | #32 design reference | #32 | Retain as design reference only | Pure extraction; see §4 |
| `src-tauri/src/engine_launch.rs` | New (206) | Does not exist | Separate product issue (#60) | #60 | Reimplement cleanly later; separate follow-up | Behavior change; see §4 |
| `src-tauri/src/pipeline_deadline.rs` | New (188) | Does not exist | Separate product issue (#60) | #60 | Reimplement cleanly later; separate follow-up | Behavior change; see §4 |
| `src-tauri/src/ocr_corruption.rs` + tests | New (201+178) | Does not exist | Separate product issue (#59) | #59 | Reimplement cleanly later; separate follow-up | Behavior change; see §4 |
| `src-tauri/src/ocr_recent_lines.rs` + tests | New (164+181) | Does not exist | Separate product issue (#59) | #59 | Reimplement cleanly later; separate follow-up | Behavior change; see §4 |
| `src-tauri/src/http_config.rs` + tests | New (143+279) | Inline non-durable version in `http_server.rs` | Separate product issue (#71) | #71 | Reimplement cleanly later; separate follow-up | Behavior change; see §4 |
| `config/maintainability-baseline.json` | Modified (decomposed structure) | Monolithic structure | Candidate design for #31-#34 | #79 | Discard (this PR) | Remote baseline is authoritative; update per-lane |
| `docs/AGENT_GUIDE.md` | Modified | Current | Stale or conflicts | N/A | Discard | Remote version is authoritative |
| `docs/evidence/2026-08-05-arm64-*.json` (2 files) | New, local-only | Does not exist | **Local-only unverified evidence** | #60 | Retain as unverified reference only (not in PR) | See §5; not authoritative |
| `src-tauri/resources/OverlayHost*` (100+ WinUI3 DLLs) | Present | Not in clean clone | Generated or machine-specific | N/A | Discard | Build artifacts; generated locally |
| `src-tauri/src/config.rs` | 595 lines | 595 lines | Already present on remote | #31 | No change | Same on both trees |
| `src-tauri/src/main.rs` | 519 lines | 519 lines | Already present on remote | #31 | No change | Same on both trees |
| `src-tauri/src/http_server.rs` | 669 lines | 727 lines | Candidate design for #31 | #31 | Reimplement cleanly later | 58 lines removed; modified from stale base |
| `.claude/`, `.vscode/`, `.workbuddy/`, `.kilo/`, `.remember/` | Present | Not in fresh clone | Private or unsafe | N/A | Excluded from inventory | IDE/agent config |

### 6.2 Summary counts

| Classification | Count |
|---|---|
| Already present on remote `main` | 3 groups |
| Candidate design for #31 | 3 groups |
| Candidate design for #32 | 10 files (6 LLM extractions + 2 pipeline extractions + 2 modified hotpots) |
| Candidate design for #33 | 1 group |
| Candidate design for #34 | 4 groups |
| Separate product issue (#59/#60/#71) | 5 groups / 8 files |
| Stale or conflicts with current `main` | 1 |
| Generated or machine-specific | 1 |
| Private or unsafe to retain | 1 group (4 directories) |
| Local-only unverified evidence | 1 group (2 files) |
| Discarded | 0 candidate-code files (build artifacts and stale docs excluded) |

Recovery decisions totals:

- Retained as design reference only: **11 groups** (`commands/` directory, 6 LLM
  modules, 2 overlay/selector modules, `pipeline_notices.rs`,
  `pipeline_repeat_policy.rs`)
- Reimplement cleanly later: **7 groups** (`commands.rs`, `main.js`,
  `overlay.js`, `selector.js`, `foundry_local.rs`, `manager.rs`,
  `http_server.rs`)
- Separate follow-up (OPEN product issues): **5 groups / 8 files**
- Discarded: build artifacts, stale `AGENT_GUIDE.md`, IDE config, stale baseline
- Accepted verbatim into this PR: **0** (documentation only)

## 7. Recovery Decisions

### Accepted (documentation/metadata only)

- This recovery inventory document (revision 2).

### Retain as Design Reference Only

- `commands/` directory structure and decomposition approach (#31)
- LLM module extraction pattern: `chat_wire.rs`, `foundry_cli.rs`,
  `foundry_models.rs`, `manager_tests.rs`, `tier.rs`, `transport_errors.rs` (#32)
- `pipeline_notices.rs`, `pipeline_repeat_policy.rs` — pure extractions of
  inline remote-`main` logic (#32)
- Overlay extraction: `overlay-clip-loop.js`, `selector-dim-overlay.js` (#34)
- Evidence files: `2026-08-05-arm64-abandoned-request-frees-slot.json`,
  `2026-08-05-arm64-engine-thread-sweep.json` (unverified local reference only)

### Reimplement Cleanly Later

- All `commands.rs` decomposition (#31)
- All LLM module extraction (#32)
- All `main.js` removal and Lit/TS consolidation (#33)
- All overlay/selector extraction (#34)
- `http_server.rs` size reduction (#31)

### Separate Follow-up (existing OPEN product issues)

- `engine_launch.rs`, `pipeline_deadline.rs` → issue #60
- `ocr_corruption.rs` + tests, `ocr_recent_lines.rs` + tests → issue #59
- `http_config.rs` + tests → issue #71

These are behavior changes; they receive their own PRs, automated gates, and
fresh Windows evidence. They are not part of #31-#34 scope.

### Discarded

- Build artifacts in `src-tauri/resources/`
- IDE/agent configuration directories
- Stale documentation and baseline (remote versions authoritative)

### Unresolved Questions

- Were the local commits (`21b3697`, `34919e2`, `4aa7975`) ever pushed to any
  remote, or always local-only? (Audit found no remote trace.)
- Do the #59/#60/#71 candidate files represent complete fixes or work-in-progress?
  Their test coverage is strong but the code is untested on current `main`.
- Should the #60 evidence files be folded into the #60 follow-up issue body?

## 8. Child-Issue Plan — Planned Bounded PR Scopes

Issue #79 establishes the recovery baseline **before** implementation starts.
The rows below are **planned scopes**, not existing branches or PRs. Each lane
begins only after this recovery PR merges and #79 closes, and inspects current
`main` fresh.

| Order | Issue | Planned PR scope | Primary files | Explicit exclusions | Automated gates | Manual gate |
|---|---|---|---|---|---|---|
| 1 | #31 | Extract `commands.rs` adapters; lifecycle/window/config services; generic IPC adapters; diagnostics support | `src-tauri/src/commands.rs`, `src-tauri/src/commands/*.rs`, `main.rs`, `config.rs`, `http_server.rs`, `lib.rs` | Engine/translation internals (#32), frontend (#33), overlay/selector (#34) | format, clippy, unit, IPC integration, bridge contract | Start/stop/settings/tray/capture smoke matching pre-refactor baseline |
| 2 | #32 | Engine manifest/install/runtime services; transport; prompt/response; validation/retry; context; pipeline orchestration; translation diagnostics | `src-tauri/src/llm/*.rs`, `engine_manifest.rs`, `engine_install_transaction.rs`, `hy_mt_runtime.rs`, `pipeline_*.rs` | Generic Tauri lifecycle (#31), frontend presentation (#33) | format, clippy, unit, IPC integration | Fresh native Windows translation regression after final structural change |
| 3 | #33 | Main-window bootstrap; setup presentation; engine/OCR readiness; session controls; settings adapters; developer diagnostics | `src/scripts/main.js`, `src/ui/*.ts`, `src/entries/main.ts` | Backend state machines, overlay/selector (#34) | format, lint, typecheck, build, frontend unit, browser smoke | Tauri manual setup/start/stop/settings regression |
| 4 | #34 | Overlay/selector state, geometry, interactions, event adapters, cleanup ownership | `src/scripts/overlay.js`, `src/scripts/selector.js`, extracted overlay/selector modules | Backend capture/translation, main/setup (#33) | format, lint, typecheck, frontend unit | Mixed-DPI, drag, resize, click-through, clipping, region persistence regression |
| Final | #35 | Clean-checkout and closure of Epic #36 | None (verification only) | No structural refactor | Full `verify.ps1 -Stage All` on clean clone of merged `main` | Full Windows x64 + ARM64 matrix after all lanes merge |

Non-goals for every lane: no new features, no dependency upgrades, no UI
redesign, no behavior fixes smuggled into structural PRs, no broadening of
maintainability ceilings.

## 9. Remote Evidence Policy

Future child issues (#31-#35) **must** provide:

- A pushed branch with remote-resolving commits
- A real GitHub pull request (not a pseudo-review or local-only branch)
- Complete CI results from GitHub-hosted checks (not only local runs)
- Clean-checkout verification on the PR branch
- Before/after ratchet measurements (`config/maintainability-baseline.json`)
- Fresh named Windows evidence (architecture, build, scenario, result)
- An issue comment linking the PR and valid merge SHA

Local-only evidence from the damaged tree is not authoritative completion
evidence and cannot satisfy a manual or automated gate unless independently
reproduced from a clean remote branch. Browser mode does not prove native
Windows behavior.

## 10. Remaining Risks

1. **Accidental regression of newer `main` fixes**: The damaged tree is 196
   commits behind remote `main`. Any copied production code would revert config
   durability fixes (#69), 0.6.7 correctness release patches, in-app update
   infrastructure, and ARM64 compilation safeguards.

2. **Behavior changes hidden inside extraction**: `engine_launch.rs`,
   `pipeline_deadline.rs`, `ocr_corruption*.rs`, `ocr_recent_lines*.rs` and
   `http_config*.rs` change runtime behavior. They must stay in their own
   follow-up PRs (#59/#60/#71), not inside #31-#34 structural work.

3. **Incomplete ARM64 verification**: No evidence exists that the local
   refactoring was tested on native ARM64 Windows with live OCR and translation;
   the two local evidence files are unverified reference material.

4. **False confidence from formatting/typecheck-only validation**: Compilation
   and lint are not evidence of behavior preservation. Each lane needs
   deterministic tests and fresh Windows evidence.

5. **Old documentation contradicting live repository settings**: The damaged
   tree's `config/maintainability-baseline.json` and `docs/AGENT_GUIDE.md`
   differ from remote `main`. Remote versions are authoritative.

6. **Manual validation being skipped again**: The previous attempt claimed
   completion without fresh Windows evidence. The remote evidence policy above
   explicitly prevents this.

7. **Draft-state CI suppression**: PR #80 shows no checks until marked ready.
   This is expected GitHub behaviour for draft PRs; it is not a repo defect and
   is being resolved by marking the PR ready after this revision.
