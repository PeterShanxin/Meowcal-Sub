# Architecture

This document describes the live Meowcal Sub architecture and the reviewed
boundaries for its staged redesign. ADR-0001 owns the product decision: Tauri 2,
Rust, Windows OCR, and one app-managed Tencent HY-MT engine in normal mode.

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
wizard UI ─────┘                                      ├─ capture + Windows OCR
                                                      ├─ translation pipeline
                                                      ├─ curated engine lifecycle
                                                      ├─ persisted config
                                                      └─ native windows + IPC
```

Browser mode is an adapter-contract test surface. It does not implement or
prove Windows capture, OCR, native windows, tray behavior, or installation.

## Live pipeline

The current session path is:

```text
capture -> preprocess -> Windows OCR -> normalize/dedupe
        -> translate -> validate -> display event -> overlay
```

`commands.rs` currently coordinates much of this path. `TranslationManager`
owns translation selection, retry, context, output validation, fallback, and
diagnostics. `FoundryLocalBackend` combines CLI discovery, service lifecycle,
transport, and prompt requests. These are recorded legacy boundaries, not the
target design.

The target path keeps the stages explicit. Every result carries a typed state:
translated, source-only, rejected, transient failure, cancelled, or stale.
Source OCR is never represented as successful translation.

## Ownership boundaries

| Boundary                | Current owner                                     | Target owner and rule                                                                  |
| ----------------------- | ------------------------------------------------- | -------------------------------------------------------------------------------------- |
| Tauri commands          | `src-tauri/src/commands.rs`                       | Thin adapters only; application services own behavior                                  |
| Browser routes          | `src-tauri/src/http_server.rs`                    | Adapter parity with supported Tauri contracts; explicit `501` for native-only behavior |
| App/window lifecycle    | `src-tauri/src/main.rs`, `commands.rs`            | One lifecycle service owns restore/show ordering, tray, and shutdown                   |
| Persisted configuration | `src-tauri/src/config.rs`                         | One versioned config service owns defaults, validation, migration, and writes          |
| Capture and OCR         | `capture/`, `ocr/`, session code in `commands.rs` | Separate capture/OCR services; pipeline orchestrator owns sequencing                   |
| Translation             | `llm/manager.rs`                                  | Pipeline service owns attempts and typed outcomes; validators do not own transport     |
| Engine runtime          | `llm/foundry_local.rs`, command helpers           | Curated engine service owns manifest, install, process, health, repair, rollback       |
| Compatibility downloads | `legacy_translate_locally.rs`                    | Kept outside normal-mode adapters; legacy/developer compatibility only                  |
| Native overlay IPC      | `ipc/`, `overlay/`, `commands.rs`                 | `ipc/protocol.rs` owns payload schema; adapters do not redefine it                     |
| In-app update           | `update_handoff.rs`, `ui/update-controller.ts`    | Handoff owns what must stop before the installer runs; the manifest is generated, never hand-written |
| Main/setup UI           | Lit components and TypeScript controllers         | One reactive snapshot drives Home/setup/settings presentation; bridge adapters stay thin |
| Overlay/selector UI     | `overlay.js`, `selector.js`                       | Separate geometry/state owners with thin bridge and DOM adapters                       |

## Shared contracts

Shared contracts have one owner before parallel decomposition begins:

- Engine package metadata: `config/engine-manifest.v1.json` is embedded into
  the application and interpreted only by `engine_manifest.rs`. Remote refresh
  is disabled; ADR-0002 owns the authenticity and update policy.
- Configuration: Rust `config` is canonical. Frontend code may present or
  submit settings but cannot invent defaults, migrations, or readiness rules.
- Commands and events: Rust payload types and `ipc/protocol.rs` are canonical.
  `tauri-bridge.js` is the single frontend transport adapter. A command or event
  change updates Rust, bridge mapping, browser parity or explicit limitation,
  and contract tests in the same pull request.
- Engine install state: `engine_install_transaction.rs` owns the versioned
  active/last-known-good record, candidate promotion, interrupted-install
  recovery, and rollback. `engine_preflight.rs` owns Windows, RAM, and storage
  compatibility checks. UI modules never infer readiness from process names,
  ports, or model IDs.
- Engine execution policy: the embedded manifest selects acceleration per
  architecture. ARM64 runs the Adreno OpenCL path with full layer offload and
  the KV cache pinned to the CPU (`gpuLayers: 99` + `--no-kv-offload`); x64
  retains Vulkan acceleration. The pairing is mandatory: on the tested
  llama.cpp b10155 + Qualcomm Adreno/OpenCL driver combination, offloading
  the KV cache to the GPU is associated with permanent hangs after minutes of
  sustained load and multi-second stalls (measured 2026-08-09, issue #60),
  while the KV-on-CPU configuration ran a sustained session without hangs.
  Runtime code cannot replace this evidence-backed policy with a global
  GPU-layer default. The evidence covers one machine, so the ARM64 GPU policy
  is gated (`engine_gpu_gate.rs`) on the validated Adreno X1-85; any other
  ARM64 GPU, and any GPU launch that never becomes healthy, runs the previous
  CPU policy instead - translation on CPU beats an unusable accelerator. The
  manifest also limits the app-owned server to one request slot; subtitle
  translation is serialized intentionally to avoid the ARM64 runtime's
  unstable automatic multi-slot latency. This remains subject to x64 and
  capture-to-overlay validation.
- Product version: `src-tauri/tauri.conf.json` is the product version record.
  `package.json` and `src-tauri/Cargo.toml` are synchronized mirrors.
- Display state: the pipeline owns translated/source-only/failure semantics.
  The overlay renders the supplied state and cannot relabel OCR as translation.
- OCR language tags: `ocr::language` owns Windows alias normalization at the
  WinRT boundary; UI availability matching and migrated config values therefore
  share the same `zh-CN`/`zh-Hans-*` compatibility contract.
- Pipeline ordering: `pipeline_session.rs` owns monotonic session/capture IDs.
  Region changes and stop requests invalidate in-flight work; Rust and frontend
  consumers reject stale results. Completed frames log privacy-safe
  capture/OCR/model/overlay/total timings without subtitle text.
- Subtitle evaluation: `subtitle_eval.rs` and
  `evals/subtitle-eval-v1.json` own the deterministic validator contract and
  opt-in live engine gate. Reports exclude source and translated text while
  recording architecture, engine/model identity, output shape, decisions, and
  latency.
- Frontend rendering: ADR-0003 owns Vite, TypeScript, and Lit for migrated
  main/setup surfaces. `app-controller.ts` owns their asynchronous UI snapshot;
  components render it and dispatch intent. Overlay and selector retain their
  existing owners until #34 migrates them deliberately.

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
