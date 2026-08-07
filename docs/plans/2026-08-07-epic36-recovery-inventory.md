# Epic #36 Recovery Baseline Inventory

**Date**: 2026-08-07 02:00+08
**Author**: recovery automation (issue #79)
**Status**: revision 4 — post-baseline reconciliation against current `main`
(2026-08-07). Sections 1-10 remain the historical audit, true at the recovery
baseline `f8c62d4`; section 11 records the current-main reconciliation and
supersedes §3.2, §3.3, §4 and §6 where they conflict with today's repository
state.
**PR**: #80 (ready for review, not merged)

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

### 3.2 Remote CI status (baseline-era record, superseded by §11.7)

> This subsection records the CI situation at the recovery baseline `f8c62d4`.
> The repository's CI architecture has since changed (issue #82) and the
> reconciliation in §11.7 describes the current contract. Statements here about
> an "ongoing" incident or a pending `windows-latest` run describe the state of
> the world at the time of the audit, not today.

GitHub reported **no status checks and no workflow runs** for PR #80 across
heads `b8bd48f08e52fb62c7b5cd2d086be7f40024e793` through
`01fdf94dda866496d658820808c0dd7ae4e206f7`.

What is known about the workflow configuration:

- The repository's CI workflow (`.github/workflows/test.yml`) triggers on
  `pull_request` and on `push` to `main`. It has no path filter, so a docs-only
  change would normally still run it, and it contains no draft-state condition
  or guard.
- A bare `pull_request:` trigger uses the default activity types `opened`,
  `synchronize`, and `reopened`. Marking a draft PR ready emits the
  `ready_for_review` activity, which is not included in the default types, so
  marking ready alone does not dispatch a run for this workflow.
- PR #80 initially received no GitHub Actions workflow run despite an `opened`
  event and subsequent branch updates. The repository workflow does not exclude
  draft pull requests, so the original cause remains undetermined.
- After the PR became ready, a new synchronization occurred during an active
  GitHub Actions incident affecting workflow dispatch and webhook delivery.
  GitHub Status reported the incident starting 2026-08-06 15:22 UTC
  (<https://www.githubstatus.com/incidents/qcvjkzcs7j74>): "Workflow runs are
  still failing, and jobs may remain queued for an extended period before
  starting or may time out"; "Webhook deliveries may be delayed." That incident
  is a plausible explanation for the missing post-ready run, but causation is
  not established with certainty.

Remote CI remains pending and must pass before merge. No claim is made that
GitHub will replay an earlier webhook after the incident clears; a fresh
supported event (a legitimate `synchronize` from a new commit, or `reopened`
via close/reopen if necessary) may be required to trigger the runs.

### 3.3 Verification table

| Evidence | Environment | Commit | Result |
|---|---|---|---|
| Clean-main local baseline | Windows 11 ARM64 (26200), Rust 1.93, Node 22 | `f8c62d4450...` | Pass (all 16 stages) |
| Earlier verified local run (revision 2) | Windows 11 ARM64 (26200), Rust 1.93, Node 22 | `01fdf94dda866496d658820808c0dd7ae4e206f7` | Pass (277 Rust unit, 16 IPC, 201 frontend, 4 browser smoke, 0 vulnerabilities) |
| Final correction head local verification | Windows 11 ARM64 (26200), Rust 1.93, Node 22 | exact final head — see PR #80 body and issue #79 comment | Recorded only in PR/issue metadata after the run; not embedded in the commit itself |
| GitHub PR checks | GitHub Actions `windows-latest` (baseline era) | latest head at baseline | Pending — no run created; see §3.2 (superseded by §11.7) |

Local `verify.ps1` results are **not** remote CI. They are a clean-checkout
baseline and a docs-only-delta confirmation; only the repository's CI (now
self-hosted — see §11.7) counts as remote CI. A commit is only reported as
verified if the full command was run on that exact commit.

## 4. Content and Provenance Audit — the ten blanket-discarded Rust files

All ten files are **tracked**, introduced **only** by the local-only consolidated
commit `21b3697` (2026-08-07, local), and did **not** exist on remote `main` **at
the recovery baseline `f8c62d4`**. Four of the ten have since landed on `main`
through #60/#71 and six remain absent from `main` but are carried by in-flight
draft PR #81 — see §11.2 for the current-main disposition of each file.
Their working-tree state at audit time was clean (no uncommitted edits). None of
the ten is a pure extraction in the sense of duplicating remote `main` code
except where noted; several are behavior-changing fixes for OPEN product issues
#59/#60/#71.

| File/group | Damaged-tree Git status | Provenance | Semantic purpose | Behavior impact | Remote-main equivalent | Classification | Target | Decision | Reason |
|---|---|---|---|---|---|---|---|---|---|
| `src-tauri/src/engine_launch.rs` (206) | Tracked | `21b3697` only | Engine worker-thread policy: `worker_threads` (cores−4, floor 4), `launch_args` appends `--threads` on ARM64 only, honours a manifest-pinned thread count | **Changes behavior** on ARM64 (adds `--threads`); tied to evidence `2026-08-05-arm64-engine-thread-sweep.json` | None on remote `main` (no `--threads`/thread-count logic anywhere) | Separate product issue — behavior-changing ARM64 engine runtime policy | #60 (exclusive) | Reimplement cleanly later in #60; do not copy | Changes ARM64 runtime thread policy; must not enter structural refactor #32 |
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

> Baseline-era inventory (at `f8c62d4`). The "Remote `main` state" column
> records what remote `main` contained at the recovery baseline. Rows whose
> remote-main state has since changed are annotated `[current main: …]`; the
> full current-main view is §11.6. The damaged-tree line counts are retained for
> provenance and are not trusted as authoritative.

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
| `src-tauri/src/llm/foundry_local.rs` | 1175 lines (modified) | 1719 lines `[current main: 1700 — further modified by #60]` | Candidate design for #32 | #32 | Reimplement cleanly later | 544 lines removed; restructured on stale base |
| `src-tauri/src/llm/manager.rs` | 946 lines (modified) | 1234 lines `[current main: 1021 — slimmed by #60]` | Candidate design for #32 | #32 | Reimplement cleanly later | 288 lines removed; restructured on stale base |
| `src-tauri/src/llm/chat_wire.rs` | New (129) | Did not exist at baseline `[current main: present — introduced by #60]` | Candidate design for #32 | #32 | Retain as design reference only | Extracted wire format |
| `src-tauri/src/llm/foundry_cli.rs` | New (335) | Does not exist | Candidate design for #32 | #32 | Retain as design reference only | Extracted CLI config |
| `src-tauri/src/llm/foundry_models.rs` | New (227) | Does not exist | Candidate design for #32 | #32 | Retain as design reference only | Extracted model definitions |
| `src-tauri/src/llm/manager_tests.rs` | New (302) | Did not exist at baseline `[current main: present — introduced by #60]` | Candidate design for #32 | #32 | Retain as design reference only | Tests for refactored manager |
| `src-tauri/src/llm/tier.rs` | New (83) | Does not exist | Candidate design for #32 | #32 | Retain as design reference only | Tier abstraction |
| `src-tauri/src/llm/transport_errors.rs` | New (136) | Does not exist | Candidate design for #32 | #32 | Retain as design reference only | Error type extraction |
| `src-tauri/src/pipeline_notices.rs` | New (164) | Inline equivalent in `commands.rs` | #32 design reference at baseline `[current main: absent; in-flight product code in draft PR #81 (#59)]` | #59/#81 | Retain as design reference only at baseline; see §11.3 | Pure extraction at baseline; PR #81 now carries it as product code |
| `src-tauri/src/pipeline_repeat_policy.rs` | New (92) | Inline equivalent in `commands.rs` | #32 design reference at baseline `[current main: absent; in-flight product code in draft PR #81 (#59)]` | #59/#81 | Retain as design reference only at baseline; see §11.3 | Pure extraction at baseline; PR #81 now carries it as product code |
| `src-tauri/src/engine_launch.rs` | New (206) | Did not exist at baseline `[current main: present — introduced by #60]` | Separate product issue (#60) | #60 | Reimplement cleanly later; separate follow-up | Behavior change; see §4 |
| `src-tauri/src/pipeline_deadline.rs` | New (188) | Did not exist at baseline `[current main: present — introduced by #60]` | Separate product issue (#60) | #60 | Reimplement cleanly later; separate follow-up | Behavior change; see §4 |
| `src-tauri/src/ocr_corruption.rs` + tests | New (201+178) | Did not exist at baseline `[current main: absent; in-flight draft PR #81 (#59)]` | Separate product issue (#59) | #59 | Reimplement cleanly later; separate follow-up | Behavior change; see §4 |
| `src-tauri/src/ocr_recent_lines.rs` + tests | New (164+181) | Did not exist at baseline `[current main: absent; in-flight draft PR #81 (#59)]` | Separate product issue (#59) | #59 | Reimplement cleanly later; separate follow-up | Behavior change; see §4 |
| `src-tauri/src/http_config.rs` + tests | New (143+279) | Did not exist at baseline (inline non-durable version in `http_server.rs`) `[current main: present — introduced by #71]` | Separate product issue (#71) | #71 | Reimplement cleanly later; separate follow-up | Behavior change; see §4 |
| `config/maintainability-baseline.json` | Modified (decomposed structure) | Monolithic structure | Candidate design for #31-#34 | #79 | Discard (this PR) | Remote baseline is authoritative; update per-lane |
| `docs/AGENT_GUIDE.md` | Modified | Current | Stale or conflicts | N/A | Discard | Remote version is authoritative |
| `docs/evidence/2026-08-05-arm64-*.json` (2 files) | New, local-only | Does not exist | **Local-only unverified evidence** | #60 | Retain as unverified reference only (not in PR) | See §5; not authoritative |
| `src-tauri/resources/OverlayHost*` (100+ WinUI3 DLLs) | Present | Not in clean clone | Generated or machine-specific | N/A | Discard | Build artifacts; generated locally |
| `src-tauri/src/config.rs` | 595 lines | 595 lines | Already present on remote | #31 | No change | Same on both trees |
| `src-tauri/src/main.rs` | 519 lines | 519 lines | Already present on remote | #31 | No change | Same on both trees |
| `src-tauri/src/http_server.rs` | 669 lines | 727 lines `[current main: 669 — slimmed by #71]` | Candidate design for #31 | #31 | Reimplement cleanly later | 58 lines removed; modified from stale base |
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

- This recovery inventory document (revision 3).

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
| 2 | #32 | Engine manifest/install/runtime services; transport; prompt/response; validation/retry; context; pipeline orchestration; translation diagnostics. **Scope recalculated against current `main` in §11.6**: `engine_launch.rs`/`pipeline_deadline.rs` are done (#60 on `main`); `llm/chat_wire.rs`/`llm/manager_tests.rs`/`manager.rs` slimming landed (#60); `pipeline_notices.rs`/`pipeline_repeat_policy.rs` are in-flight in PR #81 (#59), not #32 | `src-tauri/src/llm/*.rs`, `engine_manifest.rs`, `engine_install_transaction.rs`, `hy_mt_runtime.rs`, `pipeline_*.rs` | Generic Tauri lifecycle (#31), frontend presentation (#33), and **all behavior-changing engine-runtime work such as `engine_launch.rs`, which belongs exclusively to #60** | format, clippy, unit, IPC integration | Fresh native Windows translation regression after final structural change |
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
- Complete CI results from the repository's self-hosted CI
  (`[self-hosted, Windows, ARM64, meowcal-ci]` — see §11.7; not only local runs)
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

7. **Remote CI still pending**: PR #80 has received no GitHub Actions workflow
   run so far. The workflow does not exclude draft pull requests, so the
   original cause of the missing draft-era runs is undetermined; the GitHub
   Actions incident is a plausible but not certain explanation for the missing
   post-ready run (see §3.2). Remote CI must pass before merge.

---

## 11. Post-baseline reconciliation against current `main` (2026-08-07)

**Revision 4.** Current `main` has moved materially since the original audit.
This section reconciles the recovery baseline with the current repository state
as of today's `origin/main`, and supersedes earlier sections wherever they
conflict. The historical audit in sections 1-10 is preserved as the record of
what was true at the recovery baseline; it is not rewritten.

### 11.1 SHAs and integration

| Item | SHA |
|---|---|
| Original recovery baseline (fresh clone, 2026-08-07 02:08+08) | `f8c62d4450eafd408180e4f076b92b15981b2200` |
| Current `origin/main` used for this reconciliation | `12180d090736d84baf94387b5cd294d660b629a1` |
| Merge commit integrating `origin/main` into `recovery/epic36-remote-baseline-79` (`git merge --no-ff origin/main`) | `5ccdf2b1f1a7710941399694af29a21c156cb3b6` |
| PR #80 head before this inventory revision | `6c564a88fc0dc56158f29806f142d2196b4f4362` (revision-3 head) |
| Final PR #80 head after this reconciliation | recorded in the PR body and #79 comment |

The merge was a normal, non-rewriting merge of `origin/main` into the recovery
branch. No force push, no rebase, no history rewrite. No production files were
modified by the merge or by this revision.

### 11.2 What landed after the baseline

Nine commits reached `origin/main` between `f8c62d4` and `12180d0`; four are
first-parent integration merges:

| Commit | Change | Issue |
|---|---|---|
| `457c7f7` | Give the engine measured headroom and bound the line it is working on | **#60 implementation** (merge of `recover/phase-b-engine-headroom`) |
| `05a0985` | Give browser dev mode the app's config durability | **#71 implementation** (merge of `recover/dev-mode-config-durability`) |
| `cfa2322` | Move CI and Windows packaging to self-hosted runners | **#82** (merge of `ci/self-hosted-runners`) |
| `12180d0` | Make on-demand foreground the normative runner operating model | #82 follow-up (docs) |

### 11.3 The ten audited files: baseline vs current `main`

The revision-3 audit said all ten files were absent from remote `main`. That
statement was true at `f8c62d4`; it is **no longer true of current `main`**.
Four of the ten have landed on `main` through the #60/#71 implementation
merges. The remaining six are still absent from `main` and are carried by the
in-flight #59 draft PR #81.

| File | State at recovery baseline (`f8c62d4`) | State on current `main` (`12180d0`) |
|---|---|---|
| `src-tauri/src/engine_launch.rs` | Absent | **Present** — introduced by `fabd70c` (#60 implementation) |
| `src-tauri/src/pipeline_deadline.rs` | Absent | **Present** — introduced by `fabd70c` (#60 implementation) |
| `src-tauri/src/http_config.rs` | Absent | **Present** — introduced by `a93c230` (#71 implementation) |
| `src-tauri/src/http_config_tests.rs` | Absent | **Present** — introduced by `a93c230` (#71 implementation) |
| `src-tauri/src/ocr_corruption.rs` | Absent | Absent from `main`; in-flight product code in draft PR #81 (#59) |
| `src-tauri/src/ocr_corruption_tests.rs` | Absent | Absent from `main`; in-flight product code in draft PR #81 (#59) |
| `src-tauri/src/ocr_recent_lines.rs` | Absent | Absent from `main`; in-flight product code in draft PR #81 (#59) |
| `src-tauri/src/ocr_recent_lines_tests.rs` | Absent | Absent from `main`; in-flight product code in draft PR #81 (#59) |
| `src-tauri/src/pipeline_notices.rs` | Absent | Absent from `main`; in-flight product code in draft PR #81 (#59) |
| `src-tauri/src/pipeline_repeat_policy.rs` | Absent | Absent from `main`; in-flight product code in draft PR #81 (#59) |

**Net disposition on current `main`: 4 present (implemented), 6 absent (in
draft PR #81, unmerged).** The claim that all ten are absent is retired for the
current repository state.

### 11.4 #59 / PR #81 status

- Issue **#59 is OPEN**; its fixes are represented by **draft PR #81** (branch
  `feature/improve-quality`, head `921fee062d9f8fbbc2d5906c3cdda60aaadbb56a`),
  which is **open, draft, and not merged**.
- PR #81 carries `ocr_corruption.rs` + tests, `ocr_recent_lines.rs` + tests,
  `pipeline_notices.rs` and `pipeline_repeat_policy.rs` as product code, plus
  the rest of the #59 lane (band window/verdict, `output_validation`,
  `event_payloads`, `ocr_gate`, `translation-display`, etc.).
- The revision-3 audit classified `pipeline_notices.rs` and
  `pipeline_repeat_policy.rs` as #32 design references (pure extractions of
  inline `main` logic). PR #81 has since claimed them as in-flight #59 product
  code. They are **not on `main`** and must not be treated as merged; #32 must
  not plan to re-extract them as if they were still design-reference-only.
- **Do not claim #59 draft-PR code is already on `main`.** PR #81 is an
  in-flight product branch, not authoritative `main` state.

### 11.5 #60 and #71: implementation-on-main vs issue state

Implementation has landed; the issues remain open because their
manual-validation gates are not complete. These are different facts and are
recorded separately.

| Issue | State | Implementation on `main` | Issue still open? |
|---|---|---|---|
| #60 | OPEN | **Yes** — `engine_launch.rs`, `pipeline_deadline.rs` and related structural extractions (`llm/chat_wire.rs`, `llm/manager_tests.rs`, `llm/manager.rs` slimming, `llm/mod.rs`, `llm/foundry_local.rs`, `pipeline_translation.rs`, `event_payloads.rs`, overlay surfaces) at `fabd70c` (`457c7f7` merge) | Yes — manual validation gates not closed |
| #71 | OPEN | **Yes** — `http_config.rs`, `http_config_tests.rs`, `http_server.rs` slimmed, `config.rs`/`config_store.rs`/`config_save.rs`, `playwright.config.mjs` at `a93c230` (`05a0985` merge) | Yes — manual validation gates not closed |

The two evidence files audited in §5 were folded into the #60 implementation
commit as `docs/evidence/2026-08-05-arm64-*.json`; the engine-thread-sweep
corresponds to the shipped `engine_launch.rs` policy and the abandoned-request
file corresponds to the shipped `pipeline_deadline.rs`.

### 11.6 Current-main scope implications for #31/#32

Anything already extracted or implemented by #60/#71 must **not** be planned for
reimplementation merely because the old damaged tree contained a similar file.

**#32 (LLM / engine / pipeline structural extraction):**

- `engine_launch.rs` and `pipeline_deadline.rs` are **done on `main`** (#60).
  They are out of #32's reimplementation scope entirely.
- The LLM extraction pattern is **partially landed** via #60:
  `llm/chat_wire.rs` and `llm/manager_tests.rs` now exist on `main`;
  `llm/manager.rs` was slimmed 1234 → 1021 lines; `llm/foundry_local.rs` was
  further modified (1719 → 1700). #32 must start from these current files.
- Still design-reference-only (not on `main`): `llm/foundry_cli.rs`,
  `llm/foundry_models.rs`, `llm/tier.rs`, `llm/transport_errors.rs` — the
  damaged-tree versions remain untrusted reference material.
- `pipeline_notices.rs` / `pipeline_repeat_policy.rs` are **not** available for
  #32 re-extraction as if unclaimed: they are in-flight product code in PR #81
  (§11.4). Whether their extraction shape still informs #32 is a decision for
  #32's lane, against current `main`'s `commands.rs` inline logic, once #79
  closes.

**#31 (commands/lifecycle/config):**

- `commands.rs` remains monolithic (2066 lines) and `main.js` remains 1987
  lines on current `main` — the #31/#33 targets are unchanged.
- `http_server.rs` is now 669 lines on `main` (727 at baseline), slimmed by
  #71's `http_config.rs` extraction. #31's planned `http_server.rs` size
  reduction is partly realized; the remaining #31 scope must be recalculated
  against the current file.

**#33 / #34:** unchanged by the post-baseline work; see sections 6-8.

### 11.7 New CI architecture (issue #82, resolved incident)

The GitHub Actions dispatch incident is **resolved**; GitHub explicitly stated
that some push/pull-request events missed during the incident cannot be
replayed automatically. A fresh supported event was therefore required for
PR #80, which the reconciliation push provides (`synchronize`).

Current normal CI is **not** `windows-latest` and is **not** GitHub-hosted.
Issue #82 is closed and its migration is live on `main`. The current contract,
read from the actual `.github/workflows/test.yml`:

| Aspect | Current contract |
|---|---|
| Runner selector | `[self-hosted, Windows, ARM64, meowcal-ci]` for all three jobs |
| `Lint & Format` | Lints the ARM64 target (`aarch64-pc-windows-msvc`) and the x64 target (`x86_64-pc-windows-msvc`) |
| `Tests` | Executes the ARM64 target natively and the x64 target under Windows x64 emulation |
| `Frontend & Browser` | Once, on the native host (system Chrome channel) |
| GitHub-hosted Windows fallback | None — no expression selects `windows-latest`/`windows-11-arm`; jobs queue when no runner is online |
| PR concurrency | `pull_request` runs cancel superseded PR runs (`concurrency` group per ref, `cancel-in-progress` for PR events); `main` pushes are not cancelled |
| Runner operating model | On-demand foreground, permanently registered, deliberately not a Windows service (`docs/SELF_HOSTED_RUNNERS.md`) |
| Setup/status tooling | `scripts/setup-self-hosted-runner.ps1` (`-Mode Status` / `-Mode Check`; never `-Mode Install` for routine work) |

**Evidence language.** The document now distinguishes three verification
classes, and the current Windows CI is referred to as the repository's
self-hosted CI, never "GitHub-hosted CI":

1. **Historical local verification** — `verify.ps1 -Stage All` on the baseline
   (`f8c62d4`, 277 Rust unit / 16 IPC / 201 frontend / 4 browser smoke) and the
   revision-3 head (`6c564a8`, same gate). Recorded in §3 and the #79 comment.
2. **Current final-head local verification** — `verify.ps1 -Stage All` on the
   reconciled final head (SHA and result recorded in the PR #80 body and #79
   comment after this revision commits).
3. **Current self-hosted remote CI** — the PR #80 run on the `meowcal-ci`
   self-hosted runner (run ID and results recorded in the PR #80 body and #79
   comment).
