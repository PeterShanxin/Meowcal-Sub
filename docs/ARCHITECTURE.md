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

### Session sequence and event ordering

1. **Start.** Home's **Start translation** opens the selector with
   **Confirm and start**. `area_selection.rs` correlates that confirmation, the
   controller saves settings and waits for engine readiness, and
   `start_translation` begins the session.
2. **Capture loop.** `commands.rs` shows the overlay, opens the session's OCR
   engine in Core's OCR process, starts the translation workers, then repeats:
   capture, OCR, band filter, eligibility gate, and `Translator::observe`.
3. **Translation.** One inference worker takes the oldest queued cue that has
   no result. The worker never runs two cues at once.
4. **Presentation.** A display loop checks the FIFO head every 25 ms and emits
   a line or a clear when the reading rules below allow it.
5. **Stop.** Stop, a region change, or display loss invalidates the session or
   translation generation; see `session_lifecycle.rs` and `display_session.rs`.

Every subtitle display event is a `translation-update` carrying `sessionId`,
`captureId`, `cueId`, and `updateId`. `pipeline_session.rs` issues session,
capture and update IDs; the subtitle queue issues cue IDs.
`updateId` increases for every displayed line, clear, and notice in a session.
The overlay accepts an event only from a newer session, or from the same session
with a higher `updateId` (`pipeline-update.js`). Notices are emitted only while
the queue is idle, so they cannot overtake or displace a queued subtitle.

The target path keeps the stages explicit. Every result carries a typed state:
translated, source-only, rejected, transient failure, cancelled, or stale.
Source OCR is never represented as successful translation.
An active cue retries after a cooldown when transport or engine recovery can
resolve its failure. Deterministic output rejection does not retry the same cue.

### Timeouts, cancellation, and recovery budgets

| Operation | Budget | On expiry or failure |
| --- | --- | --- |
| One subtitle translation (`pipeline_deadline.rs`) | 5 s; backends get 4.6 s so the fallback can return | The cue reports engine slow, the line on screen stays, and the cue may retry while its source is visible. The abandoned Core request drains under Core's own deadline. |
| OCR frame (`ocrRecognizeBgra`) | 30 s in Core; the app waits 2 s longer | `OCR_TIMEOUT` ends that OCR process; the next frame starts a new one. The third consecutive recognition failure emits an error `capture-status`, and the overlay shows **Translation is interrupted.** |
| `ocrInitialize`, `ocrLanguages` | 5 s | Error to the caller; the session does not start OCR. |
| Inference recovery (`recoverInference`, `ready` retry) | 120 s, serialized | New and queued translations are skipped while it runs; failure starts the 30/60/120/300 s retry schedule. |
| Core `ready` | 110 s in Core | `READY_TIMEOUT`; the app discards that Core process. |
| Core `complete` | `timeoutMs` 1–90000 | Error reply; the app's own deadline is shorter. |
| Update metadata check | 10 s | The check fails; download and installation have no such deadline. |
| Healthy empty recognition | 15 s | The overlay shows **Waiting for subtitles…** until text returns. |
| Display probe | every 250 ms | A changed or lost capture screen stops the session and hides the overlay. |

Stop waits for the capture task to exit before reporting success. Region
changes and Stop advance generations instead of waiting for in-flight work;
late results from an old generation are discarded in Rust and in the overlay.

## Configuration, storage, and build isolation

| Data | Location |
| --- | --- |
| Settings | `%APPDATA%\<identifier>\config.json`, with `config.bak.json` as the last known-good copy (`config_store.rs`, `config_save.rs`) |
| Session logs | `%APPDATA%\<log identifier>\logs\meowcal-sub_<dev|prod>_<date>_<time>_<pid>.log`, one file per launch; `MEOWCAL_LOG_DIR` overrides the folder |
| Core engines and models | `%LOCALAPPDATA%\Meowcal\Core\sub1\<profile>\<version>\<architecture>`; Store builds use the package `LocalCache` instead |
| Band diagnostics | Only where `MEOWCAL_BAND_LOG` points |

The identifier is `com.meowcal.sub` for direct releases, `com.meowcal.sub.dev`
for development runs that load `src-tauri/tauri.dev.conf.json`,
`com.meowcal.sub.store` for Store builds, and `com.meowcal.sub.store.dev` for
debug Store builds. An identifier ending in `.dev` selects Core's `development`
profile; any other selects `production`. The log identifier comes from
`app_profile.rs` and follows the Rust build profile instead: a release-profile
build writes logs under `com.meowcal.sub` even with the development config.
At startup the app deletes `.log` files in that folder older than seven days.

`logging_settings.rs` owns the persisted `logging.level` (`off`, `basic`,
`verbose`, default `basic`) and live event filtering. Tauri and HTTP settings
apply it only after a successful save; startup applies it after config load.
Early startup diagnostics use Basic. Environment filters cannot opt into captured
content or override Off. Each event and filename carries the runtime Tauri
identifier's dev/prod label, independent of the build-profile directory choice.

`diagnostic_artifacts.rs` writes opt-in evidence off the async executor. Capture
and OCR records share session/capture identifiers; translation records include
those identifiers plus cue ID. It saves original capture PNGs, actual OCR input
PNGs with raw OCR output, and final translation text. It never serializes engine
request headers, credentials, or prompts. Artifacts can contain private screen
content and are not uploaded. `logs/verbose` has a 256 MiB total payload budget,
pruned oldest-first before every write, not only startup. An OS file lock
serializes writers in the same directory on blocking worker threads, with a
two-second lock wait limit. A two-slot
semaphore remains owned by each blocking write even if its caller is cancelled,
so repeated Stop/Start cannot create an unbounded queue of detached disk tasks. Only owned
record directories containing known regular files are pruned. Unexpected files
are preserved and stop the write. Oversized records are skipped. Disabling
Verbose does not delete existing evidence. This feature is not yet released.

Ordinary `.log` files have no size-based rotation.

The browser backend reads `%APPDATA%\<log identifier>\config.json` through
`http_config.rs`. `src-tauri/src/bin/subtitle-eval.rs` reads
`%APPDATA%\com.meowcal.sub\config.json` unless `--config` is given.
[`CONTRIBUTING.md`](../CONTRIBUTING.md#build-modes-and-user-data) maps each
development and packaging command to the data it uses.

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
  restarts. Core does not restart a failed engine on its own. While a session
  runs, a subtitle that needs the engine triggers a new `ready` request after
  30, 60, 120, and then every 300 seconds of consecutive failure; a ready
  result resets that schedule. HTTP health alone does not prove translation
  correctness. See [Core's API contract](../core/README.md#inference-recovery).
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
  `ui/update-notes.ts` extracts up to three short highlights for both
  `build-updater-manifest.mjs` and the Settings view. The view renders text and
  bold labels through Lit, never release-supplied HTML. Full notes open the
  canonical GitHub release for the available version.
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

### Cue identity and FIFO lifecycle

`subtitle_queue.rs` and `subtitle_queue/observation.rs` own one identity per
subtitle from stable OCR through translation and display.

- **Identity key.** Whitespace is collapsed, case is ignored, trailing sentence
  punctuation is removed, curly apostrophes become `'`, and 净/凈/淨 count as one
  character. Digits, word boundaries and every other CJK character change
  remain significant. There is no fuzzy matching.
- **Enqueue.** A new key becomes a cue only after two readings with that key,
  the second captured later and at most two seconds after the first, both
  eligible. The cue is enqueued with its first reading's capture time and text
  of the confirming reading. A single changed reading cannot replace the
  active cue.
- **Pause deduplication.** Readings that match the active cue only refresh it.
  A paused video therefore keeps one cue and creates no new work, even after
  that cue's translation has left the queue. Ineligible readings can keep the
  active cue alive but cannot create a cue.
- **Cue end.** A confirmed new cue ends the previous cue at the new cue's first
  reading. Empty readings end it after a quiet period of twice the starting
  sampling period, bounded to 250–900 ms, measured on capture times. OCR
  processing time does not count as empty time. When the same text returns
  after its cue ended, it is a new cue and is translated again; identical lines
  with no empty gap between them stay one cue.
- **Dequeue.** One worker takes the oldest cue without a result. Each attempt
  is bounded by the five-second translation deadline (`pipeline_deadline.rs`).
  A translated result is shown; a source-only, stopped, or no-text result
  clears the display; other states keep the current line.
- **Held-cue retry.** When the active cue's result was engine warming, engine
  slow, or a recoverable engine or transport failure, the cue is enqueued again
  once 2.5 seconds have passed, provided its source is still on screen and the
  cue is not already queued. Deterministic output rejection is not retried.
- **Invalidation.** A region change resets the queue and advances its
  generation, so late results from older work are dropped. Stop and display
  loss end the session and discard all queued work.
- **Exceptional skipping.** Ordinary backlog is kept and measured. Only a
  severe stall, 32 queued cues or a 15-second-old queue head, drops every
  queued cue except the cue still on screen, counts them as skipped, logs
  `Subtitle queue recovered from a severe stall`, and clears the display.
- **Metrics.** Every five seconds the display loop logs depth, oldest age,
  enqueued, completed, displayed and skipped counts, and input and output
  rates. Depth above three or an oldest age above three seconds is logged as
  falling throughput.

### Display duration

`subtitle_timing.rs` decides how long a translated line stays before the next
completed cue may replace it. Reading time starts when the translation becomes
visible, so a late result still gets time on screen.

- The source budget is the cue's on-screen duration minus one measured sampling
  interval, bounded to 350–4000 ms. The subtracted interval absorbs the
  uncertainty of a sampled end time.
- The reading estimate is 400 ms plus 35 ms per non-CJK character and 70 ms per
  CJK character, bounded to 600–3500 ms.
- The minimum is 250 ms plus 25 ms per non-CJK and 50 ms per CJK character,
  bounded to 350–1200 ms and never above the source budget.
- The hold is the larger of the source duration and the reading estimate, minus
  the display delay, bounded by that minimum and the source budget.

Because the source budget caps every hold, long translations cannot build a
growing display backlog when inference keeps up. While the source is still on
screen its duration keeps growing, so a paused line stays visible. After the
source ends, the line clears once it has been readable and nothing is queued.
A queued successor that is still translating keeps the previous line for up
to four seconds after that line's source ended, instead of inserting a blank.
Text replacement has no per-cue slide animation.

### Adaptive sampling

`pipeline_pacing.rs` calibrates the capture period for each session. The
starting period is the configured `captureIntervalMs` (250 ms by default; no UI
control exposes it). A new
`Pacer` starts with every translation session; region changes do not reset it.

- **Observation window.** Each successful capture and OCR pass records its
  capture-plus-OCR time. The window keeps the latest 24 samples and makes no
  adjustment before 12 samples exist, so start-up and one slow frame do not
  change the period.
- **Target.** Twice the window's 90th-percentile cost, bounded below by half
  the configured interval (never under 60 ms) and above by the configured
  interval. The calibrated period never exceeds the configured interval; only
  work that overruns the period makes sampling slower.
- **Step.** Each sample moves the period toward the target by at most one
  eighth of the current period.
- **Over budget.** When a frame's work exceeds the period, the next capture
  starts immediately. Recognition overruns already slow sampling, so no extra
  pacing penalty is added. Failed captures and OCR errors wait out the current
  period and add no sample.
- **Measured interval.** Each cue carries the actual time since the previous
  successful capture, including overruns and outages. The display duration
  uses that interval for the sampled-end reserve, and stage logs report it.

The queue's quiet period is fixed from the starting period when the session
begins. Sampling never pauses for the queue: capture and OCR continue while a
cue waits for translation or display, which is how a paused cue stays active
and a new cue can be confirmed during inference. A period change is logged as
`pipeline_sampling_calibrated` at debug level.

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
