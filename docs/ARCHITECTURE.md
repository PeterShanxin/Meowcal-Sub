# Architecture

This document describes the live Meowcal Sub architecture and the reviewed
boundaries for its staged redesign.
[ADR-0001](adr/0001-curated-local-translation-stack.md) owns the product
decision: Tauri 2, Rust, and Tencent HY-MT in normal mode;
[ADR-0007](adr/0007-embedded-pp-ocr-recognizer.md) makes PP-OCRv6 in Core the
default recognizer, with Windows OCR selectable.
[ADR-0004](adr/0004-versioned-meowcal-core.md) defines the independent Core
runtime shared with Meowcal Sub 2. [`adr/README.md`](adr/README.md) says which decisions belong in
an ADR and which belong here.

## Runtime shape

The desktop process owns the application lifecycle, Windows integration, and
translation session. Four webviews provide the setup/main window, selector,
subtitle overlay, and setup wizard. Vite builds them as a static multi-page
frontend. Browser development mode serves the same entries and maps bridge
calls to a loopback-only Rust HTTP adapter.

```text
main/setup UI ─┐
selector UI ───┼─ TauriBridge ─ commands / HTTP adapters ─ application services
overlay UI ────┤                                      │
wizard UI ─────┘                                      ├─ capture + OCR policy
                                                      ├─ translation pipeline
                                                      ├─ Core process adapters
                                                      ├─ persisted config
                                                      └─ native windows + IPC
```

Browser mode is an adapter-contract test surface. It does not implement or
prove Windows capture, OCR, native windows, tray behavior, or installation.
When a reviewed Core lock exists, its browser backend fetches that release and
builds without the source-candidate feature. `npm run dev:backend:source-candidate`
is the explicit local-Core path.

## Meowcal Core

`core/` builds independently of the Tauri application. Its versioned Windows
executable owns HY-MT artifacts, installation/recovery, execution policy, model
requests, and native OCR. Both applications use the same JSON API over inherited
standard input/output. They pin a Core version and reject an incompatible
handshake. The Rust and Python consumers are transport adapters, not alternative
engine implementations.

OCR runs in a separate Core process from inference. Each application retains
its capture and preprocessing pipeline, including Sub 1's frame budgets and
line geometry and Sub 2's corroborated white-text pass. Core does not select a
subtitle band, match a subtitle file, or decide which text belongs on screen.

Core storage is partitioned by client, profile, version, and architecture
([ADR-0005](adr/0005-client-partitioned-core-storage.md)). Cross-process leases
prevent installation from replacing files in use. Other Core partitions supply
verified assets as hard links; legacy import copies, leaving the previous
application installation available for rollback. `core/src/storage_reclaim.rs`
removes a client's stale versions and links identical models. Neither
application's update changes the other's pin.

## Live pipeline

The current session path is:

```text
capture -> OCR in Core -> band/text eligibility -> stable cue FIFO
        -> serial translation/validation -> readable FIFO presentation -> overlay
```

`commands.rs` coordinates capture and recognition. `subtitle_queue.rs` and its
observation module own stable cue identity, pending work, the active source and
display lifetime. `pipeline_translation.rs` runs one inference worker while
`pipeline_translation/presentation.rs` presents completed cues in FIFO order.
`TranslationManager`
owns backend selection, fallback, context storage, and diagnostics;
`llm/translation_planner.rs` owns the context-tier progression state machine
(degradation on timeout/slow success, persisted effective tier) above one
attempt; `llm/translation_attempt.rs` owns the single-backend attempt/transient-retry
state machine (request, validation invocation, typed outcome, attempt-level
diagnostics). `FoundryLocalBackend` combines CLI discovery, service lifecycle,
and the compatibility façade. Managed HY-MT requests use
`llm/core_translation.rs` and the Core process adapter. Compatibility HTTP
execution and API-namespace discovery remain in `llm/transport_http.rs`
(`HttpTransport`).

The target path keeps the stages explicit. Every result carries a typed state:
translated, source-only, rejected, transient failure, cancelled, or stale.
Source OCR is never represented as successful translation.

## Ownership boundaries

| Boundary                   | Current owner                                                                                                                                                                                                         | Target owner and rule                                                                                                                                                                        |
| -------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Tauri commands             | `src-tauri/src/commands.rs`                                                                                                                                                                                           | Thin adapters only; application services own behavior                                                                                                                                        |
| Browser routes             | `src-tauri/src/http_server.rs`, `http_port.rs`                                                                                                                                                                        | Adapter parity with supported Tauri contracts; explicit `501` for native-only behavior. `http_port.rs` owns the listening port so verification can take a free one instead of requiring 3001 |
| Application state          | `src-tauri/src/app_state.rs`                                                                                                                                                                                          | One owner for shared session state and the region/monitor/DPI invariants                                                                                                                           |
| App/window lifecycle       | `src-tauri/src/main.rs` (composition root), `window_lifecycle.rs`, `tray.rs`, `app_logging.rs`, `selector_window.rs`, `wizard_window.rs`                                                                              | One lifecycle service owns restore/show ordering, tray, and shutdown                                                                                                                         |
| Persisted configuration    | `src-tauri/src/config.rs`, `capture_region.rs`, `settings_service.rs`                                                                                                                                                 | One versioned config service owns defaults, validation, migration, and writes                                                                                                                |
| Capture and OCR            | `capture/`, `ocr/` (product policy), `core/src/ocr/` (native recognition), `ocr_language_packs.rs` (Windows pack setup)                                                                                                                                            | Separate capture/OCR services; pipeline orchestrator owns sequencing                                                                                                                         |
| Subtitle lifecycle | `subtitle_queue.rs`, `subtitle_queue/observation.rs`, `subtitle_timing.rs` | One owner for cue identity, FIFO work and readable presentation |
| Translation eligibility    | `translation_eligibility.rs`                                                                                                                                                                                          | One owner for whether the pipeline judges subtitle shape; `ocr::BandFilter` and `ocr_gate` read it, neither decides it                                                                       |
| Translation                | `llm/manager.rs` (selection/fallback/context storage), `llm/translation_planner.rs` (context-tier progression), `llm/translation_attempt.rs` (attempt/retry state machine)                                            | Pipeline service owns attempts and typed outcomes; validators do not own transport                                                                                                           |
| Translation transport      | `core_client` and `llm/core_translation.rs` (managed HY-MT); `llm/transport_http.rs` (compatibility endpoints)                                                                                                                                                                             | Namespace discovery, endpoint URL assembly, and GET/POST dispatch with 404 fallback; no retry/context/validation policy                                                                      |
| Engine runtime             | `core/src/` (manifest/install/process/recovery), `core_client` and `engine_status` (app adapters), `llm/foundry_local.rs` (compatibility façade)                                                           | Curated engine service owns manifest, install, process, health, repair, rollback; adapters stay thin                                                                                         |
| Compatibility downloads    | `legacy_translate_locally.rs`                                                                                                                                                                                         | Kept outside normal-mode adapters; legacy/developer compatibility only                                                                                                                       |
| Native overlay IPC         | `ipc/` (protocol, server, handler), `overlay/`, `commands.rs`                                                                                                                                                         | `ipc/protocol.rs` owns payload schema; adapters do not redefine it                                                                                                                           |
| In-app update              | `update_handoff.rs`, `ui/update-controller.ts`                                                                                                                                                                        | Handoff owns what must stop before the installer runs; the manifest is generated, never hand-written                                                                                         |
| Main/setup UI              | Lit components and TypeScript controllers                                                                                                                                                                             | One reactive snapshot drives Home/setup/settings presentation; bridge adapters stay thin                                                                                                     |
| Overlay/selector geometry  | `region-geometry.js` (capture-region move/resize, shared), `overlay-geometry.js` (frame DPI tokens, subtitle placement, clip rounding), `selector-geometry.js` (selection rectangle, dim segments, persisted payload) | One owner per rule, pure and testable without a WebView; no window owns a private copy                                                                                                       |
| Overlay presentation state | `overlay-appearance.js` (font/colour/toggle defaults, hydrate and patch), `overlay-timers.js` (named-slot owner for the click-through poll, frame fade, and hide cleanup)                                             | One rule per concern; a timer scoped to overlay state is cancelled by the state change, not by the page ending                                                                               |
| Overlay/selector UI        | `overlay.js`, `selector.js`                                                                                                                                                                                           | Adapters only: DOM, Tauri IPC, and interaction state on top of the geometry and state owners                                                                                                 |

## Shared contracts

Shared contracts have one owner before parallel decomposition begins:

- Engine package metadata: `core/config/engine-manifest.v1.json` is embedded into
  Core and interpreted by `core/src/engine_manifest.rs`. Remote refresh is
  disabled. ADR-0002 defines manifest authenticity; ADR-0004 transfers the
  release and implementation boundary to Core.
- Context safety: recent-line context remains off by default. When enabled,
  `llm/context_leakage.rs` keeps up to 12 accepted source/target pairs in
  session memory solely for comparison; targets never enter a prompt or disk.
  If a contextual answer repeats a different source's whole translation,
  `llm/translation_planner.rs` discards it and retries with no context under
  the original deadline, retaining the no-context tier for the session.
  Punctuation and case differences are ignored; unchanged source cues and
  matching fragments inside Unicode words are exempt. CJK answers shorter than
  four characters require a whole-answer match. Scene gaps expire source context
  and comparison history before the next prompt is built. Scene age uses the
  last valid OCR observation independently of history eviction or compression.
  Reset advances the
  comparison generation, so an in-flight request from the previous scene cannot
  repopulate history. Even output rejected by the generic validator is
  checked for replay before choosing fallback. Managed contextual length
  rejections retain their raw output and recovery receipt until replay is
  classified. A wrong-language answer gets one immediate retry with an explicit
  target-language instruction and no context, within the original attempt and
  time budgets. Exhausted rejection reports to Core, and stronger corruption
  signals retain immediate recovery. The retry can confirm that two
  different cues legitimately share a translation. This is a replay
  guard, not a semantic detector for paraphrases or previously unseen leakage.
- Configuration: Rust `config` is canonical. Frontend code may present or
  submit settings but cannot invent defaults, migrations, or readiness rules.
  Live read-modify-write saves hold the `AppState.config` lock through the
  durable write and publication. Settings and capture updates use
  `config_save::update_live` on blocking workers; capture state is published
  before releasing that lock so a settings save reads the committed selection.
- Commands and events: Rust payload types and `ipc/protocol.rs` are canonical.
  `tauri-bridge.js` is the single frontend transport adapter. A command or event
  change updates Rust, bridge mapping, browser parity or explicit limitation,
  and contract tests in the same pull request.
  `src-tauri/tests/command_contracts.rs` pins command payload shapes from
  outside `src/`, so it keeps passing unchanged while implementations move
  between modules. `get_system_info` is deliberately snake_case there; the rest
  of the surface is camelCase.
- Engine install state: `core/src/engine_install_transaction.rs` owns the versioned
  active/last-known-good record, candidate promotion, interrupted-install
  recovery, and rollback. `core/src/engine_preflight.rs` owns Windows, RAM, and storage
  compatibility checks. UI modules never infer readiness from process names,
  ports, or model IDs.
- Engine execution policy: the embedded manifest selects acceleration per
  architecture. ARM64 runs the Adreno OpenCL path with full layer offload and
  the KV cache pinned to the CPU (`gpuLayers: 99` + `--no-kv-offload`); x64
  requests Vulkan offload (`gpuLayers: 99`). The pairing is mandatory: on the tested
  llama.cpp b10155 + Qualcomm Adreno/OpenCL driver combination, offloading
  the KV cache to the GPU is associated with permanent hangs after minutes of
  sustained load and multi-second stalls (measured 2026-08-09, issue #60),
  while the KV-on-CPU configuration ran a sustained session without hangs.
  Runtime code cannot replace this evidence-backed policy with a global
  GPU-layer default. The evidence covers one machine, so the ARM64 GPU policy
  is gated (`engine_gpu_gate.rs`) on the validated Adreno X1-85 plus driver
  31.0.148.0, with at least 4 GiB of available memory and of available commit
  at launch. Any other ARM64 GPU or driver, a launch below that headroom, and
  any GPU launch that never becomes healthy or fails the sample translation,
  runs the previous CPU policy instead - translation on CPU beats an unusable
  accelerator. A driver update returns to CPU until new
  sustained evidence deliberately expands the allowlist. The x64 Vulkan
  runtime uses its GPU policy only when DXCore reports a discrete hardware
  adapter: on the measured host the integrated GPU was slower than the CPU
  (docs/plans/2026-11-07-x64-vulkan-gpu-benchmark.md). Other x64 hosts run the
  CPU policy, and a failed x64 GPU start retries on CPU in the same way. Core's
  `acceleration` status reports the policy that launched the engine. The
  manifest also limits the app-owned server to one request slot; subtitle
  translation is serialized intentionally to avoid the ARM64 runtime's
  unstable automatic multi-slot latency. This remains subject to x64 and
  capture-to-overlay validation.
- Inference correctness: Core's `inference_health` owns bounded response receipts
  and independent failure evidence; `inference_recovery` owns probing and verified
  CPU fallback. Sub 1 validates raw output before sanitization can hide debris,
  then reports rejection through `core_client_recovery`. That adapter serializes
  recovery, skips stale requests, and retains CPU policy across internal Core
  restarts. CPU recovery failure requires explicit retry. HTTP health alone does
  not prove translation correctness. See [Core's API contract](../core/README.md#inference-recovery).
- Readiness refresh never prepares a busy Core. It retains a previously confirmed
  ready snapshot during translation. Queued and active installation invalidates
  readiness and rejects older status writes. Without confirmed readiness the app
  reports `busy`; the UI follows it with one bounded status recheck at a time.
  In-flight preparation is shared and cleared on success and failure.
  Confirming a subtitle area in the native app prewarms an installed, stopped
  engine while idle, provided settings have finished saving. Start joins that
  preparation before refreshing readiness. Restored areas and cancelled selectors
  do not trigger prewarming. This moves model loading earlier, including its memory
  use; capture and translation still wait for Start.
  Focus refresh preserves the stopped state during owned preparation so Start
  stays available. A speculative preparation failure reports its error while
  retaining the stopped state for retry. An engine already reporting `preparing`
  still transitions to `error` on preparation failure.
- Product version: `src-tauri/tauri.conf.json` is the product version record.
  `package.json` and `src-tauri/Cargo.toml` are synchronized mirrors.
- Update checks: `ui/update-controller.ts` owns a ten-second metadata deadline.
  Manual checks join an existing automatic check's remaining deadline. Late
  results cannot replace a retry, and unused update resources are closed.
  Download and installation do not use the metadata deadline.
  `update_diagnostics.rs` records source, timing, result, and error category in
  the session log without raw errors or request URLs. Browser mode has no updater;
  Store builds keep Microsoft Store as their update owner.
- Display state: the pipeline owns translated/source-only/failure semantics.
  The overlay renders the supplied state and cannot relabel OCR as translation.
  `subtitle_activity.rs` owns the 15-second healthy-empty waiting threshold,
  independently of subtitle retirement and repeat reset. Capture or recognition
  failures interrupt an unfinished timer; an existing wait lasts until source
  text returns. `pipeline_status.rs` emits activity and terminal
  session reports; `ui/session-status-controller.ts` filters stale reports and
  preserves actionable interruptions on Home. `overlay-session-status.js` shows
  a neutral waiting indicator after 15 seconds of healthy empty recognition and
  keeps it visible until source text returns. Each later gap starts a fresh
  15-second interval. Waiting, preparation and actionable interruptions use a
  surface outside the subtitle box. Short gaps and successful retries stay quiet.
- Selection confirmation: `area_selection.rs` correlates each Start with an
  explicit selector confirmation or cancellation. It delegates display validation
  and persistence to the display-session owner. An unchanged rectangle confirms
  normally; cancellation preserves the committed selection. Capture starts only
  after the selector is hidden and the confirmed selection has been saved.
- Display targeting: `capture_target.rs` owns monitor identity, local geometry,
  pending selector tokens and restore validation. `display_monitor.rs` combines
  Tauri physical bounds/DPI with the Windows monitor device interface identity.
  Opening the selector pins the monitor with the largest intersection with Home.
  Selector and overlay coordinates are monitor-local logical pixels. WGC crops
  monitor-local physical pixels; GDI adds the unscaled physical desktop origin.
  Overlay hit tests also add the physical origin after scaling local coordinates.
- Display lifecycle: `display_session.rs` rejects Start after an idle move to a
  different monitor. A running session retains its selected monitor. A 250 ms
  session-scoped probe detects identity/bounds/DPI changes independently of OCR
  and inference; capture also checks the target and native overlay bounds before
  and after each frame. A moved overlay stops capture instead of reading another screen.
  Display loss invalidates in-flight work, stops capture and hides the overlay,
  then requires a new selection. Reconnection does not restart translation.
  Saved areas require matching screen identity, bounds, DPI and current Home
  monitor; legacy areas restore only on a single-display desktop.
  An app-owned one-second topology watch updates Home after idle hotplug and
  stops at application exit.
  `ui/display-controller.ts` refreshes display context after debounced Home moves
  and handles display-loss notices without probing the translation engine.
- Overlay liveness: `overlay-ready` and `overlay-heartbeat` are emitted by the
  overlay frontend and owned by `overlay/liveness.rs`, which distinguishes a
  live native window from a renderer still consuming events. A stale renderer
  is recovered exactly once per show (stale-clip reset + one WebView reload,
  bounded wait); a native clip reset to an empty safe region always precedes a
  show so an old subtitle clip cannot hide a new session.
- OCR language tags: `ocr::language` owns Windows alias normalization at the
  WinRT boundary; UI availability matching and migrated config values therefore
  share the same `zh-CN`/`zh-Hans-*` compatibility contract.
- Pipeline ordering: `pipeline_session.rs` owns monotonic session/capture IDs.
  Region changes and stop requests invalidate in-flight work; Rust and frontend
  consumers reject stale results. Completed frames log privacy-safe
  capture/OCR/model/overlay/total timings without subtitle text.
- Session teardown: `session_lifecycle.rs` serializes Start and Stop through
  capture-task cleanup and overlay hiding. OCR waits observe Stop. A successful
  Stop means the old capture task has exited; bounded teardown failure keeps
  Stop retryable and cannot authorize a second running capture session.
- Overlay exit: `overlay/commands.rs::exit_translation` awaits that Stop before
  `window_lifecycle.rs` restores and focuses the main window. The main controller
  consumes `translation-exited` to return to Home with a stopped session;
  browser mode returns `501`. `overlay-toolbar.js` owns control visibility,
  pending/error presentation, and DOM wiring over the shared geometry rules.
- Overlay keyboard input: `overlay/window_focus.rs` enables native activation
  after show and cursor-style changes. Tao remains non-focusable during those
  operations so they cannot activate the window; explicit click or Alt+Tab
  reveals the toolbar and gives its controls keyboard focus.
  After show, register the named Translation controls window with the shell
  switcher; clearing the native no-activate flag alone does not register it.
  Native close requests invoke the same guarded exit action, preserving the
  overlay renderer for the next session.
- Subtitle evaluation: `subtitle_eval.rs` and
  `evals/subtitle-eval-v1.json` own the deterministic validator contract and
  opt-in live engine gate. Reports exclude source and translated text while
  recording architecture, engine/model identity, output shape, decisions, and
  latency.
- Frontend rendering: [ADR-0003](adr/0003-incremental-lit-frontend.md) owns
  Vite, TypeScript, and Lit for migrated main/setup surfaces.
  `app-controller.ts` owns their asynchronous UI snapshot;
  components render it and dispatch intent. Overlay and selector keep their own
  owners: #34 closed after establishing the geometry, appearance, and timer
  owners listed above, and deliberately left `overlay.js` and `selector.js` as
  adapters rather than migrating them to Lit.

## Subtitle admission and presentation

`ocr/band_cue.rs` confirms text changes against a fixed content anchor.
Band stability uses monotonic capture times, so uneven OCR completion cannot
expire an otherwise consecutive pair of source images.
Spatial scatter rejects wandering scene text; unchanging bands retain static
watermark history. Confirmed dialogue is not rejected for changing rapidly or
for changing a number. Two identical changed readings also release a static
anchor after a pause; a single changed reading retains it.
Pure numeric readings remain untranslatable.
Rapid names in credit sequences are translated too. The raw turnover
categories remain diagnostics only.
Single Chinese characters pass the text gate. `Translate any text` remains an
Advanced option for menus and pages; it bypasses subtitle-shape rules while
retaining stable cue confirmation and FIFO ordering.

The subtitle queue confirms consecutive normalized readings. Normalization
keeps word boundaries, digits and apostrophes, while treating 净/凈/淨 as one
character. Other CJK character changes remain significant. A transient changed read cannot
replace the active identity. A confirmed new cue or sustained empty source ends
the previous cue. OCR processing time does not count as observed empty time.
A paused cue remains active after its completed work leaves the queue.

Inference proceeds before source disappearance. Busy inference retains pending
cues. Presentation starts as soon as the FIFO head completes and the previous
line has had a reading interval. Source duration, translated length and delay
set that interval. The source duration caps the reading budget, bounded to
350–4000 ms, so longer translations cannot create a persistent display backlog.
A length-based minimum up to 1200 ms is limited by the same budget. One sampling
interval is reserved for source-duration uncertainty. A pending successor can
retain the previous line briefly, up to four seconds, instead of inserting a
blank flash. Text replacement
has no per-cue slide animation.

Each cue keeps its ID through translation and display. `captureId` identifies the
source observation; `updateId` orders display events within a session, including
clears. Region changes and session stop invalidate queued and active work.
The queue logs input/output rates, depth, oldest age and skip counts every five
seconds. Depth above three or age above three seconds reports falling throughput.
Only a severe stall (32 queued entries or a 15-second queue age) skips old work
and resumes from the newest cue. Normal backlog is retained and measured.

OCR pacing starts at the configured interval. After 12 successful samples, a
rolling p90 capture/OCR cost can shorten it to half that interval, with bounded
per-frame changes. Complex frames do not add idle time beyond the configured
cadence; recognition overruns already slow sampling. Reading budgets and stage
logs use the measured interval between successful captures, including outages.

Band diagnostics omit screen text. `MEOWCAL_BAND_LOG` enables local JSONL;
`MEOWCAL_BAND_LOG_TEXT=1` additionally includes OCR text for replay. Keep those
files private. Stage logs and character counts cannot establish real-media
ground truth. `evals/realtime-subtitles/run.ps1` compares the queue with the
pre-FIFO policies under deterministic timing; native gates verify the full path.

## Dependency direction

Native and HTTP adapters depend on application services; services depend on
domain contracts; platform and runtime implementations satisfy those
contracts. Domain code must not depend on Tauri window objects, DOM objects, or
HTTP route types. Frontend state/presentation helpers must not invoke the
backend directly.

Long-running work has explicit ownership and cancellation. Child processes are
identified by exact handles/PIDs and only app-owned or explicitly adopted
processes may be stopped. Blocking disk, hash, process, and Windows operations
must not stall UI or async executor threads.

## Staged decomposition ownership

The maintainability epic deliberately assigns non-overlapping production areas:

- #31: `commands.rs`, generic lifecycle/window/config services, generic IPC
  adapters, and support diagnostics.
- #32: engine install/runtime, transport, prompt/response, validation/retry,
  context, pipeline, and translation diagnostics.
- #33: main/setup frontend, OCR/engine readiness presentation, session controls,
  settings adapters, and developer diagnostics.
- #34: overlay/selector state, geometry, interactions, event adapters, and
  cleanup ownership.

Each lane may update shared contract tests and normative documentation, but it
must not redefine another lane's state machine. A required shared-contract
change is reviewed first or delivered as a separate prerequisite.

## Change rules

- Structural pull requests preserve visible behavior. A visible fix receives a
  separate scope and fresh manual Windows validation.
- New production files stay within the reviewed ceiling in
  `config/maintainability-baseline.json`.
- Existing hotspots may shrink, but cannot grow beyond their recorded ceiling.
- Cross-cutting product decisions require an ADR; routine implementation detail
  belongs here or beside the owning module.
- The authoritative verifier and maintainability baseline must change in the
  same pull request as any approved contract or threshold change.

See `docs/MAINTAINABILITY_BASELINE.md` for measured ceilings, coverage scope,
ratchet behavior, and update procedure.
