# #32 Wave 2 — Translation HTTP transport boundary

**Date:** 2026-08-11
**Parent:** #36
**Issue:** #32 (partial)
**Starting main:** `123c319a447b522f556c3f858d0e77d1f9b88200`
**Branch:** `refactor/32-engine-pipeline-wave2`
**Worktree:** `C:\FormerD\Repos\wt-meowcal-32-wave2`

## Goal

Extract the HTTP translation **transport** — endpoint request execution,
namespace discovery, and transport-level response acquisition — out of
`FoundryLocalBackend` into one new #32-owned owner, `llm/transport_http.rs`.

`FoundryLocalBackend` keeps its legacy CLI discovery, service lifecycle,
probe cache, model selection, prompt/request construction, response parsing,
and the `TranslatorBackend` compatibility façade.

This is a **structural** wave. It does not change generation parameters,
timeouts, retries, prompts, error strings, or any observable behavior.

## Non-goals

- No `TranslationManager` retry/attempt/context move (candidate B — later wave)
- No capture/translate session-loop move from `commands.rs` (candidate C — later)
- No Foundry product-contract rename
- No HTTP managed-runtime parity decision
- No retry redesign, no context redesign
- No #103 / #105 / #107 / #60 recovery behavior
- No GPU/runtime launch policy change
- No new full-text logging; no removal of existing local debug logs

## Candidate comparison (A / B / C)

| Criterion | A: transport extraction | B: manager attempt extraction | C: capture-loop extraction |
| --- | --- | --- | --- |
| Ownership clarity | High — one state machine (namespace discovery) | Medium — retry/tier/diagnostics/budget interleave | Medium — session/notices/stop channels interleave |
| Behavior sensitivity | Low–moderate | High | Highest |
| Current characterization | Near zero (one trivial test) | Good mock infra in `manager_tests.rs` | Partial |
| State machines touched | 1 | 2 (context tier + attempt) | 3+ |
| Async/cancellation risk | Low (no locks across await) | Medium (`sleep` in retry loops) | High |
| Public-contract exposure | None (all private) | None | None |
| Manual-test cost | Small smoke (translate path touched) | None | High |
| Hotspot reduction | `foundry_local.rs` 1700 → ~1510 | `manager.rs` 1021 → ~730 | `commands.rs` 1209 → ~900 |
| Usefulness for later #32 | Directly unblocks B and C seams | Marginal for A | Marginal until A/B done |
| Rework chance | Low | Moderate | High |

**Selection: A.** The seam is clean — all transport callers live inside
`foundry_local.rs`; the namespace state machine and dispatch are self-contained;
the move is behavior-preserving with a byte-identical error mapping table
(section 6); and the durable boundary (translate / list_models / probe against a
loopback endpoint) is strongly characterizable without a real engine.

## What moves (current → target)

| Cluster | Current owner | Target owner |
| --- | --- | --- |
| B. HTTP namespace/endpoint discovery (`API_NAMESPACE_*`, `preferred_api_namespace`, `api_url_for`, `fallback_namespace`, `api_namespace` state) | `foundry_local.rs` | `llm/transport_http.rs` |
| C. HTTP request transport (client construction, `get_with_namespace_fallback`, `post_with_namespace_fallback`, probe client dispatch, `check_health` GET) | `foundry_local.rs` | `llm/transport_http.rs` |
| A. Legacy CLI discovery/service lifecycle (statics, `service_url`, `service_available`, `cached_models`, refresh/ensure/mark) | `foundry_local.rs` | stays |
| M. Probe cache + snapshot + phase | `foundry_local.rs` | stays |
| D. Request serialization | `chat_wire.rs` (DTOs) + call sites | unchanged |
| E. Response deserialization | `chat_wire.rs` (DTOs) + `.json()` at call sites | unchanged |
| F. Model listing (HTTP + CLI fallback + dedup + cache) | `foundry_local.rs` | stays (calls transport for the HTTP part) |
| G. Prompt construction | `prompt_router.rs` | unchanged |
| H. Output validation | `output_validation.rs` | unchanged |
| I. Transient classification | `transport_errors.rs` | unchanged; `TransportError` preserves message text so `is_transient` is unaffected |
| J. Attempt retry policy | `manager.rs` + port-change retry in translate | stays |
| K. Context-tier degradation | `manager.rs` | unchanged |
| L. Backend fallback | `manager.rs` | unchanged |
| N. Session/cancellation/stale | `pipeline_session.rs` / `pipeline_translation.rs` / `commands.rs` | unchanged |
| O. Thin adapters | `commands.rs` / `http_server.rs` / `TranslatorBackend` impl | unchanged |

## Target ownership

```text
FoundryLocalBackend  ──►  HttpTransport (llm/transport_http.rs)
  legacy CLI discovery       client (timeout from config.timeout_ms)
  service lifecycle          api_namespace state machine
  probe cache / phase        api_url_for + namespace fallback dispatch
  model selection            get/post_with_namespace_fallback
  prompt/request building    probe_models (per-call probe client)
  response parsing           check_health
  port-change retry          reset_namespace
  TranslatorBackend façade
```

Dependency direction: `foundry_local` → `transport_http` → `transport_errors`
(classification unchanged), `chat_wire` (DTOs unchanged). The transport does
**not** own retry policy, fallback, context tiers, output validation, engine
installation, GPU launch, or health state.

## New module contract

`src-tauri/src/llm/transport_http.rs` (new production file, ≤ 400 lines):

```rust
pub(super) enum TransportError {
    Timeout(reqwest::Error),   // reqwest error where is_timeout() == true
    Failed(reqwest::Error),    // any other reqwest-level failure
    ApiStatus(u16),            // HTTP response with non-success status
}

pub(super) enum ModelsProbeOutcome {
    Ready,                     // 200 (direct or fallback namespace)
    TimedOut,                  // request hit its timeout
    RequestFailed(reqwest::Error),
    BadStatus(u16),
}

pub(super) struct HttpTransport { client: Client, api_namespace: AtomicU8 }

impl HttpTransport {
    pub(super) fn new(timeout_ms: u64) -> Self;
    pub(super) async fn get_with_namespace_fallback(&self, base_url: &str, endpoint: &str)
        -> Result<reqwest::Response, TransportError>;   // uses self.client
    pub(super) async fn post_with_namespace_fallback<T: Serialize>(&self, base_url: &str, endpoint: &str, body: &T)
        -> Result<reqwest::Response, TransportError>;   // uses self.client
    pub(super) async fn probe_models(&self, client: &Client, base_url: &str) -> ModelsProbeOutcome;
    pub(super) fn check_health(&self, base_url: &str) -> bool;
    pub(super) fn reset_namespace(&self);
}
```

Notes:

- `new` builds the client exactly as today: `Client::builder().timeout(...).build().unwrap_or_default()`.
- The probe path passes its own client (built per call with the probe timeout)
  but shares the backend's `api_namespace` state, exactly as today: a successful
  probe teaches the transport which namespace works, and the next translation
  request uses it without a 404 round-trip.
- The transport keeps the current internal `debug!` logs verbatim
  ("Endpoint ... returned 404, trying fallback ...", "Foundry Local probe
  succeeded", "... (fallback)", "Foundry Local probe timed out").

## Error semantics (BEFORE → AFTER, byte-identical)

Transport no longer returns `LlmError`; `foundry_local.rs` maps
`TransportError` at its boundary with exactly the current strings and log
levels:

| Path | TransportError | Current behavior preserved by the mapping |
| --- | --- | --- |
| GET (`list_models`) | `ApiStatus(s)` | `Err(ApiError("API error {status}"))`, no warn |
| GET | `Timeout`/`Failed(e)` | `Err(ApiError(describe_request_failure(e)))`, no warn |
| POST (`translate`, `summarize_context`) | `ApiStatus(s)` | `warn!("Local translation endpoint returned HTTP {status}")` + `Err(ApiError("API error {status}"))` |
| POST | `Timeout`/`Failed(e)` | `warn!("Foundry Local request failed: {e}")` + `Err(ApiError(describe_request_failure(e)))` |
| Probe | `Ready` | `record_probe_success()` + `debug!` + `Ok(true)` |
| Probe | `TimedOut` | `record_probe_timeout()` + `debug!` + `Ok(false)` |
| Probe | `RequestFailed(e)` | `record_probe_error("Probe failed: {e}")` + `Err(ApiError("Probe failed: {e}"))` |
| Probe | `BadStatus(s)` | `record_probe_error("Models endpoint returned status {s}")` + `Err(ApiError(...))` |

`describe_request_failure` (in `transport_errors.rs`) is unchanged, so
`is_transient` classification is untouched. The transport-level error *is* the
`reqwest::Error` (kept inside `Timeout`/`Failed`/`RequestFailed`), so the probe
can still distinguish timeout from failure and `is_timeout` semantics survive
the move.

## Timeout semantics (unchanged)

- Default transport client: `config.timeout_ms` (per `FoundryLocalConfig`).
- Probe: per-call client with the probe's `timeout_ms` argument
  (`FAST_PROBE_TIMEOUT_MS` / `SLOW_PROBE_TIMEOUT_MS`).
- No nested timeouts added; retry/attempt caps remain in `manager.rs`;
  the `backend_budget()`/`TRANSLATION_DEADLINE` interaction is untouched.

## Request schema (unchanged)

The transport serializes the same `ChatCompletionRequest` from `chat_wire.rs`
(`model`, `messages`, `temperature`, `top_k`, `top_p`, `repeat_penalty`,
`max_tokens`) to the same endpoint paths (`/openai/v1/…` or `/v1/…` for
`chat/completions` and `models`). No field or value changes.

## Response parsing semantics (unchanged)

Response acquisition (`response.json()`) stays at the call sites in
`foundry_local.rs`; `ChatCompletionResponse`/`ChatUsage` in `chat_wire.rs` are
unchanged. `translate` still takes `choices.first().message.content`,
trims, and sanitizes; `summarize_context` keeps its own extraction; `list_models`
keeps the `ModelsResponse` parsing, CLI fallback, and dedup.

## State / cancellation implications

- `HttpTransport` holds no locks; `AtomicU8` namespace state only. No mutex
  guard crosses an `.await` in the moved code (verified: the current dispatch
  holds no guards; the new module keeps that shape).
- Cancellation behavior unchanged: dropping the translation future still drops
  the in-flight `reqwest` request (same as today).
- `mark_service_unavailable()` still resets the namespace — it now calls
  `transport.reset_namespace()` with identical effect.
- The port-change retry in `translate_with_context_options` (refresh service
  URL from CLI, retry once) stays in `FoundryLocalBackend` because it couples
  `service_url` state with CLI discovery.

## Before/after behavior matrix

Every row below is **identical before and after**; the matrix documents what
was verified, not what changed.

| Dimension | BEFORE (main 123c319) | AFTER (this wave) |
| --- | --- | --- |
| Default namespace | OPENAI (`/openai/v1/`) | OPENAI |
| 404 → fallback namespace | retry `/v1/`, store on success | same (inside transport) |
| Namespace learned by probe | shared with translate | shared (same `HttpTransport`) |
| Successful translation | 200 → parse → trim → sanitize → `record_probe_success` | same |
| Empty completion content | `Ok("")` | same |
| Malformed JSON | `ApiError("Failed to parse response: …")` | same |
| Non-success status (POST) | `warn!` + `ApiError("API error {status}")` | same |
| Non-success status (GET) | `ApiError("API error {status}")`, no warn | same |
| Reqwest failure (POST) | `warn!("Foundry Local request failed: …")` + `describe_request_failure` | same |
| Probe timeout | `Ok(false)` + `record_probe_timeout` | same |
| Probe failure | `Err(ApiError("Probe failed: …"))` + `record_probe_error` | same |
| Probe bad status | `Err(ApiError("Models endpoint returned status …"))` | same |
| `check_health` 404 quirk | store fallback namespace without verifying, return false | same (inside transport) |
| Client timeout | `config.timeout_ms` | same |
| Probe client timeout | per-call probe arg | same |
| Port-change retry | once after service refresh | same (backend) |
| `record_probe_success` after chat | only on translate success | same |
| `mark_service_unavailable` | clears URL + availability + namespace + probe cache | same |
| `is_transient` classification | string markers in `transport_errors.rs` | same |
| Logging | same targets/levels/strings (transport debug logs move with the code) | same; three debug/warn-level nuances, none visible at default log level: (1) the probe's former "Probing Foundry Local models endpoint: …" line is dropped and "Probing fallback endpoint: …" is replaced by the shared dispatch's "Endpoint … returned 404, trying fallback …" (same severity); (2) a POST fallback request that fails at the transport level now emits the same `warn!("Foundry Local request failed: …")` the direct path already emitted (previously silent); (3) the probe's success/timeout debug lines keep their exact text ("Foundry Local probe succeeded (fallback)") |
| Public API | `FoundryLocalBackend` surface | unchanged |

The only textual difference is the *file* the code lives in; no log line,
error string, status path, or classification changes.

## What explicitly did NOT move

- CLI discovery (`foundry service status`, `foundry cache list`,
  `foundry model info`, `is_cli_available`, caches, start cooldowns)
- Service lifecycle (`refresh_service_status`, `ensure_service_running`,
  `try_start_service`, stabilization wait, port-change retry)
- Probe cache, `probe_snapshot`, `determine_phase`, `phase`
- Model selection (`get_model`, `resolve_model_id`, `choose_auto_model`, …)
- Prompt construction and request building (translate + summarize)
- Response parsing, sanitization
- Retry counts/delays/classification, context tiers, fallback policy
- Session/cancellation/stale orchestration (all of `pipeline_*`, `commands.rs`)

## Characterization plan

New `src-tauri/src/llm/transport_http_tests.rs` with an in-process loopback TCP
mock (tokio only; no external network, no Foundry installation, no CLI spawn):

- endpoint construction: default namespace `/openai/v1/models`, fallback `/v1/models`
- namespace fallback on 404 is remembered (second call skips the 404)
- probe-learned namespace is used by the next translation request
- non-success status → `API error {status}` mapping (GET no warn, POST warn path)
- malformed JSON → parse error string
- missing/empty completion content → `Ok("")`
- successful status → translated text, trims + sanitize applied
- transport error preservation: refused connection message contains
  `Request failed:`; timeout classified via probe `Ok(false)`
- timeout selection: per-call probe client vs config client (wide margins)
- wire shape: request body carries model id, role/content, temperature,
  top_k/top_p, repeat_penalty, max_tokens; prompt text unmutated
- model identity/config propagation: `config.model` reaches `request.model`
- `check_health` true/false and 404 quirk
- `reset_namespace` after `mark_service_unavailable` restarts at `/openai/`
- probe cache recording via `probe_snapshot()`

Deterministic fixture assertions only; no unwrap/expect/panic on I/O or
network error paths (per CODING_STANDARDS — the mock server's bind/listen
results are propagated via `?` in `Result`-returning helpers).

## Manual validation consequences

The live translate request/response path is touched (dispatch moves out of
`FoundryLocalBackend`), so a **small owner smoke** is required before merge:

1. one clean app launch;
2. one normal subtitle translation — source → translated output succeeds;
3. one stop/quit;
4. no stress/repeat loop (minimizes #107 exposure; do not reproduce #103/#105).

No long benchmark. The final #32 wave still requires the issue-level fresh
native translation regression before closure.

## Verification

- `cargo fmt --check`, `cargo clippy --locked --lib --bins -- -D warnings`
- focused `cargo test --locked --lib` runs during development
- full `.\scripts\verify.ps1 -Stage All` before push
- `git diff --check`

## Ratchet

- `config/maintainability-baseline.json`: lower `llm/foundry_local.rs` ceiling
  from 1700 to the measured post-extraction line count (must equal measured).
- No new legacy exception; new `llm/transport_http.rs` must be ≤ 400.
- Negative proof: temporarily raise the new ceiling by one line and verify the
  maintainability gate fails, then revert (removed before commit).

## Residual #32 after Wave 2

1. `TranslationManager` attempt/retry/context orchestration (candidate B)
2. capture → OCR → translate session-loop ownership in `commands.rs` (candidate C)
3. Foundry-specific normal product contracts (naming)
4. HTTP managed-runtime parity decision
5. final native translation regression before #32 closure
