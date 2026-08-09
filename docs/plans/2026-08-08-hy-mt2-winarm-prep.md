# Hy-MT2-1.8B Windows ARM64 preparation (2026-08-08)

Status: isolated preparation/integration only. HY-MT1.5 remains the
application default. A/B quality benchmark is explicitly deferred to the next
task. This document is the handoff for that task.

Update (2026-08-09): the A/B benchmark is COMPLETE and the decision is
frozen - KEEP HY-MT1.5-1.8B-Q4_K_M as production/default; do NOT promote
Hy-MT2-1.8B-Q4_K_M yet; no user-facing model selector. Clean/general
translation quality was approximately tied, but MT2 is worse on OCR/noise
and latency tail. See
`docs/plans/2026-08-08-real-session-benchmark.md` ("Decision" /
"Methodology corrections" / "Future evaluation funnel"). The isolated MT2
path described here is retained for future targeted experiments.

## 1. Current HY-MT1.5 setup (measured on this machine)

| Item | Value |
|---|---|
| Machine | Windows 11 build 26200, Snapdragon X Elite X1E80100, 12 Oryon cores |
| Model | `HY-MT1.5-1.8B-Q4_K_M.gguf` (Tencent HY-MT1.5 1.8B) |
| Quantization | Q4_K_M |
| Artifact size | 1,133,080,512 bytes (on-disk, app-managed cache) |
| SHA-256 | `4383ac0c3c8e476de98ff979c2a3f069f8c4fb385e7860cf2d28da896cc477c7` (matches embedded manifest) |
| Runtime | llama.cpp `llama-server` release **b10155**, `llama-b10155-bin-win-opencl-adreno-arm64.zip` variant |
| Engine architecture | Native Windows ARM64, run CPU-only (`-ngl 0`) |
| Install root | `<foundry-cache-root>\meowcal-sub\` (runtime + models subdirs; machine-local) |
| App config | `%APPDATA%\com.meowcal.sub\config.json` → `translation.foundryLocal.managedRuntime` { kind hy-mt, port 11436 } |
| Packaging | Embedded manifest `config/engine-manifest.v1.json` (ADR-0001 curated stack; ADR-0002 embedded authenticity; remote refresh disabled) |
| Thread policy | `engine_threads(12) = (12-4).clamp(4,8) = 8` (`src-tauri/src/engine_launch.rs`) |
| Launch args | `llama-server -m <model> -a HY-MT1.5-1.8B-Q4_K_M --host 127.0.0.1 --port 11436 -c 2048 -ngl 0 --jinja --no-webui --parallel 1 --threads 8 -v 3` |
| Wire | POST `/v1/chat/completions` (app tries `/openai/v1/...` first → 404 → fallback) |
| Request body | `{model, messages:[user], temperature 0.7, top_k 20, top_p 0.6, repeat_penalty 1.05, max_tokens: 120}` |
| Prompt | Prompt Router single user message (CN or EN), no system prompt; context segment optional (user config: context-aware OFF) |

## 2. Hy-MT2 candidate selection

Official upstream: `huggingface.co/tencent/Hy-MT2-1.8B-GGUF` (plus separate
2-bit and 1.25-bit repositories).

| File | Size (bytes) | Note |
|---|---|---|
| `Hy-MT2-1.8B-Q8_0.gguf` | 1,908,528,192 | reference quant |
| `Hy-MT2-1.8B-Q6_K.gguf` | 1,474,785,120 | - |
| `Hy-MT2-1.8B-Q4_K_M.gguf` | **1,133,080,448** | chose this (below) |
| 2-bit / 1.25-bit variants | - | separate repos; STQ quant (ternary lattice); require unmerged llama.cpp STQ kernel (PR #22836) — deferred |

**Primary artifact: `Hy-MT2-1.8B-Q4_K_M.gguf`** — same K-quant family as the
deployed MT1.5 Q4_K_M (nearly identical size, 64 B difference), same
Q4_K_M/vocab working set, apples-to-apples for benchmark #1. Extreme
quantizations intentionally deferred to a separate question.

- SHA-256 `dc5f44fcf1fa496ee7ad725982c0c8c553a4de00259b53c8572e...` — verified
  against HF LFS `oid` on download and locally.
- GGUF metadata (b10327 `/v1/models`): `n_vocab=120818`, `n_ctx_train=262144`,
  `n_embd=2048`, estimated params 1,791,080,448, `ftype=Q4_K - Medium`,
  arch **`hunyuan-dense`**.
- Model card recommended sampling: temp 0.7, top_k 20, top_p 0.6,
  repetition_penalty 1.05 — identical to Meowcal Sub's wire constants, so no
  per-model decode change needed.
- Model card example prompt matches the app's Prompt Router shape (single
  user message, no system prompt) — no prompt/router change needed.

## 3. Windows ARM64 compatibility

- Production b10155 **cannot load `hunyuan-dense`** — a newer engine is
  required for MT2. This is a hard incompatibility, not a policy choice.
- Chosen engine: official release **b10327**
  (`llama-b10327-bin-win-opencl-adreno-arm64.zip`, published 2026-08-08),
  same release family and ARM64 packaging as the current b10155 install;
  native Windows ARM64, run CPU-only (`-ngl 0`) for parity.
- STQ kernel (`ggml-org/llama.cpp#22836`, unmerged) affects only the 2-bit /
  1.25-bit STQ quant files, NOT K-quants. Verified: stock b10327 loads and
  runs `Hy-MT2-1.8B-Q4_K_M.gguf`.
- No source build required (official binaries are sufficient; no toolchain
  needed). Virtual environment: `JIT-kernel` not used.

### Measured numbers (this machine, b10327, `-c 2048 -ngl 0`)
- Model load ~1.9 s; llama-server working set ~2.67 GB (Task Manager).
- First completion 1192 ms (zh→en, 5 tokens) incl. prompt processing.
- Warm completions ~427 ms (5 tokens) / ~681 ms (7 tokens).
- Clean exit: `Stop-Process` → port closes, no orphan processes.

## 4. Isolation model

- Production untouched: no manifest change, no app config change, no UI
  change. HY-MT1.5 remains the shipped default.
- MT2 files outside the app-managed root (machine-local isolation dir):
  - `<mt2-isolation-root>\runtime\llama-b10327-opencl-adreno-arm64\`
  - `<mt2-isolation-root>\models\hy-mt2-1.8b-q4\Hy-MT2-1.8B-Q4_K_M.gguf`
- Harness driver (experiment-only, not committed to main; the reusable
  single-config eval path is the `subtitle-eval` binary): `scripts/run-engine-eval.ps1`
  - `-Engine mt15` — copies the live app config verbatim (keeps the managed
    runtime record); `subtitle-eval` spawns the app's own b10155 engine the
    way the app would.
  - `-Engine mt2` — builds `eval-results\engine-mt2-config.json` from the app
    config with `model=Hy-MT2-1.8B-Q4_K_M`, `endpointUrl=127.0.0.1:11449`,
    `managedRuntime` removed; starts b10327 `llama-server` with app-equivalent
    args (port 11449, alias `Hy-MT2-1.8B-Q4_K_M`); waits for `/health`; runs
    `subtitle-eval --live`; stops the server.
  - Same dataset `evals/subtitle-eval-v1.json`, same body/params on the wire.
- `eval-results/` is gitignored (machine-path configs, logs, reports stay local).

## 5. Compatibility findings

Worked unchanged (b10327 + MT2 Q4_K_M):
- GGUF chat template via `--jinja`; sanitized translation outputs, no format drift.
- Wire shape incl. `max_tokens: 120` and sampling constants.
- Prompt Router EN/CN single-message prompt.
- `/openai/v1` → `/v1` namespace fallback and unknown-model-name tolerance
  (b10327 accepts an arbitrary request `model` with one model loaded).
- Context-style prompt (single user message with context header) produces
  clean output; delimiters survive.
- App's thread policy `8` reproducible by hand (see §1).

Required changes / notes:
- Engine move b10155 → b10327 for the MT2 candidate. **This is the single
  unavoidable confound** in the A/B benchmark (b10155 cannot load hunyuan-dense).
- Minor tokenizer warning on load: `special_eos_id is not in special_eog_ids`
  (b10327) — benign; EOS still terminates generation.
- MT1.5 2-bit GGUF present locally in the app cache
  (`hy-mt1.5-1.8b-2bit`) from prior experiments — untouched.

No remaining blockers for the smoke level.

## 6. Smoke / harness results (2026-08-08)

Direct wire (b10327 + MT2, same body as app):
- zh→en: "Hello, it's been a long time. How has your work been?" 1192 ms (first request).
- context-style zh→en: "He finally arrived." 427 ms.
- ja→en: "Hello. Are you well?" 681 ms.
- Repeated requests OK; parallel requests serialize (`--parallel 1`).

Harness throughput runs (Runs=1 — smoke only, NOT benchmark):

| Side | report | passed | warmup | p50 | p95 | budget | cases |
|---|---|---|---|---|---|---|---|
| MT2 | `eval-results\mt2-<stamp>.json` (local, gitignored) | true | 123 ms | 413 ms | 731 ms | within | 11 |
| MT1.5 | `eval-results\mt15-<stamp>.json` (local, gitignored) | true | 8177 ms | 2349 ms | 15584 ms | outside | 11 |

Notes:
- Both reports `schemaVersion` same; `filteredCases=1` (dataset filter), all
  other cases validated & accepted.
- MT1.5 side includes managed-runtime startup inside the warmup +
  first-request effects; not a benchmark data point.
- No leftover `llama-server.exe` after either run; both ports closed.

## 7. Benchmark readiness — exact A/B configurations for the next task

Trusted status: pipeline-level readiness **confirmed**; quality-level
benchmark intentionally NOT run.

| Dimension | MT1.5 (baseline) | MT2 (candidate) |
|---|---|---|
| Model artifact | `HY-MT1.5-1.8B-Q4_K_M.gguf` | `Hy-MT2-1.8B-Q4_K_M.gguf` |
| Quantization | Q4_K_M | Q4_K_M |
| Engine | llama.cpp b10155 (native ARM64) | llama.cpp b10327 (native ARM64) |
| Acceleration | CPU, `-ngl 0`, threads 8 | CPU, `-ngl 0`, threads 8 |
| Context | 2048 (`-c 2048`) | 2048 (`-c 2048`) |
| Port | 11436 (app-managed) | 11449 (driver-managed) |
| Wire/sampling | temp 0.7 / top_k 20 / top_p 0.6 / rep_pen 1.05 / max_tokens 120 | identical |
| Prompt | Prompt Router (CN/EN), no system prompt | unchanged (router untouched) |
| Context-aware | OFF (app config) | OFF (same app config base) |
| Dataset | `evals/subtitle-eval-v1.json` | same |
| Runner | `scripts\run-engine-eval.ps1 -Engine mt15 -Runs 5` (experiment-only) | `scripts\run-engine-eval.ps1 -Engine mt2 -Runs 5` (experiment-only) |

Documented confound: engine build version differs (b10155 vs b10327) —
forced by the new model architecture; everything else is matched as closely
as the isolated setup allows.

## 8. Repository changes

Landed to main (reusable infra only):
- `docs/plans/2026-08-08-hy-mt2-winarm-prep.md` — this document.
- `docs/plans/2026-08-08-real-session-benchmark.md` — benchmark methodology
  and frozen decision.
- `src-tauri/src/subtitle_eval.rs` and `src-tauri/src/bin/subtitle-eval.rs` —
  harness improvements (allow-unreferenced datasets, CJK-aware grading,
  per-case output capture, request-error tolerance).
- `scripts/build-crosscheck-pack.ps1`, `scripts/aggregate-blind.ps1`,
  `scripts/attribute-failures.ps1`, `scripts/analyze-bench.ps1` — reusable
  analysis tooling.

Not committed (experiment-only, kept in the benchmark worktree):
- `scripts/run-engine-eval.ps1` — eval driver for both engines (machine-local
  default paths, config-specific orchestration).

Machine-side artifacts (not in git):
- `<mt2-isolation-root>\models\hy-mt2-1.8b-q4\Hy-MT2-1.8B-Q4_K_M.gguf`
- `<mt2-isolation-root>\runtime\llama-b10327-bin-win-opencl-adreno-arm64.zip` (12,333,056 B) + extracted `llama-b10327-opencl-adreno-arm64\`

Production repo untouched: no manifest edit, no app config edit, no Rust
production-path changes, no UI changes.