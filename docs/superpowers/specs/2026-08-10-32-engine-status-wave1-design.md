# #32 Wave 1 — Engine status orchestration boundary

**Date:** 2026-08-10  
**Parent:** #36  
**Issue:** #32 (partial)  
**Starting main:** `6faf05ac941f188c97cf7ae9ce3194cc8c3a277e`  
**Branch:** `refactor/32-engine-pipeline-wave1`

## Goal

Extract engine status / readiness **orchestration** for the four operations:

- get status
- refresh status
- prepare
- make ready

into one #32-owned service. Tauri and HTTP adapters become thin mappers that
preserve their **existing** observable semantics, including intentional
Tauri↔HTTP differences.

## Non-goals

- No translation transport move
- No manager retry/validation move
- No capture/translation loop move
- No Foundry product-contract rename
- No #103 / #105 / #107 recovery work
- No GPU/runtime launch policy change
- No silent HTTP adoption of Tauri managed-runtime semantics
- No removal of existing local debug full-text logs

## Architecture

```text
Tauri commands  ──► engine_status::{*_tauri}  ──► managed branch (Tauri only)
HTTP routes     ──► engine_status::{*_http}   ──► legacy Foundry only (HTTP)
                              │
                              ├── hy_mt_runtime (managed readiness)
                              └── FoundryLocalBackend (legacy CLI/probe)
```

- Domain result type: `EngineStatusSnapshot` (engine-oriented field names).
- Wire DTOs remain adapter-local `FoundryLocalStatus` (Foundry-named).
- Phase/probe types stay the existing `FoundryLocalPhase` /
  `FoundryProbeSnapshot` for this wave (rename is a later #32 wave).
- Separate entry points per adapter (`*_tauri` / `*_http`) so profiles cannot
  silently converge.

---

## Current-behavior parity / difference matrix

Source of truth: `commands.rs` and `http_server.rs` on main `6faf05a`.

### Shared wire shape (both adapters)

Fields (camelCase on the wire):

| Field | Meaning |
| --- | --- |
| `cliAvailable` | CLI present (legacy) or executable present (managed Tauri) |
| `serviceRunning` | service URL / health |
| `serviceUrl` | endpoint if known |
| `models` | model id list |
| `configuredModel` | config model (None = Auto) |
| `selectedModel` | resolved / configured selection |
| `notes` | human notes |
| `phase` | `FoundryLocalPhase` |
| `probe` | optional `FoundryProbeSnapshot` |

### Operation: get status

| Dimension | Tauri (`get_foundry_local_status`) | HTTP (`GET /api/foundry-local/status`) | Wave-1 rule |
| --- | --- | --- | --- |
| Startup gate | `startup_gate.wait_until_ready()` before work | none | **Adapter-only** (stays in Tauri command) |
| Managed runtime branch | **Yes** — `managed_hy_mt_status(config, start_if_needed=false)` | **No** — always builds `FoundryLocalBackend` | **Preserve difference** |
| Legacy path | `spawn_blocking(build_foundry_local_status_no_probe)` | inline sync: refresh + CLI + models + `phase()` | Same legacy snapshot semantics; execution may stay blocking on HTTP |
| Probe | none | none | same |
| Probe snapshot | backend / managed `None` | backend snapshot | same |
| Join failure | `Err("Foundry Local status task failed: …")` + warn log | N/A (no spawn_blocking on this path) | preserve Tauri error |
| Return shape | `Result<FoundryLocalStatus, String>` | always `200` + JSON body | preserve |

### Operation: refresh status

| Dimension | Tauri | HTTP | Wave-1 rule |
| --- | --- | --- | --- |
| Managed branch | **Yes** (`start_if_needed=false`) | **No** | **Preserve** |
| Blocking snapshot | `refresh_service_status`, CLI available/url/models, `notes()` | same body | same |
| Probe when running && models non-empty | cache valid → Ready; else `probe_chat_completions(FAST_PROBE_TIMEOUT_MS)` → Ready/Preparing/Error | same mapping | same |
| Probe logging | debug/info/warn with detail | silent match arms | logging may stay adapter-local or move with code; must not change phases |
| Join failure | `Err("Foundry Local status task failed: …")` | soft fallback: false/empty/`"Foundry Local refresh task failed"` notes, continue | **Preserve** |
| Notes on probe outcomes | unchanged from snapshot notes | unchanged | same |

### Operation: prepare

| Dimension | Tauri | HTTP | Wave-1 rule |
| --- | --- | --- | --- |
| Managed branch | **Yes** (`start_if_needed=true`) | **No** | **Preserve** |
| Service start | `ensure_service_running()` in blocking section | same | same |
| Probe timeout | `SLOW_PROBE_TIMEOUT_MS` when running && models | same | same |
| Notes on probe Ok(true) | append `" Warmup complete."` | **no append** | **Preserve** |
| Notes on probe Ok(false) | append `" Model still warming up."` | **no append** | **Preserve** |
| Notes on probe Err | append `" Probe error: {e}"` | append `" Probe error: {e}"` | same |
| Join failure | `Err("Foundry Local prepare task failed: …")` | soft fallback notes `"Foundry Local prepare task failed"` | **Preserve** |

### Operation: make ready

| Dimension | Tauri | HTTP | Wave-1 rule |
| --- | --- | --- | --- |
| Managed branch | **Yes** (`start_if_needed=true`) | **No** | **Preserve** |
| Initial start | inside loop when `!service_running` each iteration | one-shot before loop: if `cli_available` then `ensure_service_running` + refresh | **Preserve** |
| Early exit if not ready to probe | none (loop handles NotInstalled / NotRunning / NoModels) | if `!cli \|\| !running \|\| models.empty` → return `backend.phase()` immediately | **Preserve** |
| Total timeout | 90s | 90s (probe loop only) | same constant |
| Models-empty wait | wait up to **12s** with 900ms sleep, phase `NoModels`, then break | no models-wait loop (early exit) | **Preserve** |
| Not-running handling | phase `NotRunning`, try start, sleep 900ms, continue | early exit before loop; inside loop if refresh loses service → `NotRunning` break | **Preserve** |
| Not-installed | phase `NotInstalled`, break | early exit via phase | **Preserve** |
| Probe attempt 1 timeout | `SLOW_PROBE_TIMEOUT_MS` | same | same |
| Later probe timeout | `steady.max(FAST)` where steady = `timeout_ms.clamp(5000, SLOW)` | same | same |
| Sleep between probes | 1500ms | 1500ms | same |
| Refresh inside probe loop | per-iteration blocking refresh **before** branch decisions | refresh **after** each probe attempt | **Preserve** |
| Join failure (snapshot) | `Err("Foundry Local make-ready snapshot failed: …")` | soft fallback notes `"Foundry Local make-ready task failed"` | **Preserve** |
| Notes if not Ready + last_error | append `" Last error: {err}"` | same | same |
| Notes if not Ready + no last_error | **unchanged** | append `" Still warming up. Try again shortly."` | **Preserve** |

### Managed path (Tauri only today)

| Dimension | Behavior |
| --- | --- |
| Trigger | `config.managed_runtime.is_some()` |
| `start_if_needed=false` | `hy_mt_runtime::is_healthy` when exe+model ready |
| `start_if_needed=true` | `hy_mt_runtime::ensure_ready(..., 90s)` |
| Phase map | !exe → NotInstalled; !model → NoModels; running → Ready; else NotRunning |
| Notes | fixed strings (“Local Translation Engine is ready.”, etc.) |
| `cli_available` | executable file present |
| `service_url` | always `Some(endpoint_url)` when managed branch taken |
| `models` | configured model if model_ready else empty |
| `selected_model` / `configured_model` | config.model |
| `probe` | always `None` |

HTTP must **not** gain this branch in Wave 1.

### Follow-ups (explicitly not this PR)

1. HTTP missing managed-runtime status branch (parity gap / likely bug for browser mode once managed is default).
2. Foundry-named wire contracts → generic engine contracts.
3. Unify make-ready loops after product decides which policy is correct.
4. Unify prepare warmup note strings if product wants identical UX copy.

---

## Extraction can be behavior-preserving?

**Yes**, if and only if:

- managed branching is gated by adapter profile (Tauri on / HTTP off);
- join-failure policy is per adapter;
- prepare note append policy is per adapter;
- make-ready control flow is two implementations (or one parameterized with the matrix flags), not a lowest-common-denominator merge.

No observable adapter semantic change is required for the move.

## Module layout

| Path | Role |
| --- | --- |
| `src-tauri/src/engine_status.rs` | domain snapshot + orchestration entry points |
| `src-tauri/src/engine_status_tests.rs` | characterization tests |
| `commands.rs` | thin Tauri adapters + startup_gate + DTO map |
| `http_server.rs` | thin HTTP adapters + DTO map |

New production file ≤ 400 lines. If orchestration exceeds ceiling, split
`engine_status_make_ready.rs` without changing ownership.

## Privacy

No new subtitle/screen text logging. Existing local debug full-text logs are
accepted development diagnostics and are untouched.

## Verification

- Characterization tests for matrix rows that are pure/deterministic
- `cargo fmt`, clippy, focused lib tests
- Full `.\scripts\verify.ps1 -Stage All`
- Ratchet lower `commands.rs` and `http_server.rs` to measured sizes
- Manual gate: optional status refresh smoke only (no full translation regression for this wave)

## Residual #32 after Wave 1

- `foundry_local.rs` transport/CLI still large
- `manager.rs` attempt/retry/context orchestration
- `commands.rs` capture/translate loop
- Foundry naming on product contracts
- HTTP managed parity follow-up
