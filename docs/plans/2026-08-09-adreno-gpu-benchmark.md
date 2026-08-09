# Adreno GPU Offload Benchmark: HY-MT1.5-1.8B-Q4_K_M on Windows ARM64

Status: DRAFT (measurements in progress)
Date: 2026-08-09
Scope: controlled A/B benchmark of Adreno OpenCL GPU offload vs the current
CPU-only ARM64 configuration, for issue #60. No production default was
changed; no merge was made; NPU/QNN/ONNX explicitly out of scope.

## Goal

Decide whether Adreno GPU offload (`-ngl N`) should become the default ARM64
runtime configuration (`gpuLayers` in `config/engine-manifest.v1.json`) for
the production translation model HY-MT1.5-1.8B-Q4_K_M.

## 1. Main SHA tested

- `main` HEAD at benchmark start: `856d8648e7d5e8e3faae298a2f559a977c03eef7`
- Benchmark worktree base: `integrate/benchmark-infra` @ `ee24711`
  (main + 3 benchmark-infra commits, unmerged)
- Benchmark branch: `bench/gpu-adreno-arm64` @ `e98b80c` (instrumentation
  only; NOT for merge)

## 2. Branch / worktree

- Worktree: `C:\FormerD\Repos\.wt-meowcal-gpu-bench`
- Branch: `bench/gpu-adreno-arm64`
- Untouched: main worktree, `.wt-meowcal-mt2`, `.wt-meowcal-bench-integ`,
  all uncommitted work elsewhere.

## 3. Exact runtime / version

- Model: `HY-MT1.5-1.8B-Q4_K_M.gguf` (sha256 `4383ac0c...c477c7`, matches
  manifest) at `C:\FormerD\foundry-cache\meowcal-sub\models\hy-mt1.5-1.8b-q4\`
- Runtime: llama.cpp **b10155** release
  `llama-b10155-bin-win-opencl-adreno-arm64.zip`, sha256 `1b0ead2e...9b07`
  (manifest), executable `llama-server.exe` sha256 `8cbee8b9...b8d14` (matches
  manifest) at `C:\FormerD\foundry-cache\meowcal-sub\runtime\llama-b10155-opencl-adreno-arm64\`
- Evaluation binary: `subtitle-eval.exe` release build from the benchmark
  worktree (same binary used for every arm)
- Windows: 11 Pro build 26200, ARM64. Host HONOR MRO-XXX, Snapdragon X Elite
  X1E80100, 12 cores / 12 logical, 32 GB RAM.
- GPU: Qualcomm Adreno X1-85, driver 31.0.148.0. OpenCL 3.0 QUALCOMM
  build 863.0 Compiler DX.50.39.00. Adreno LUID `0x00000000_0x00010d66`.
- Power scheme: Balanced (381b4222-f694-41f0-9685-ff5bb260df2e).

## 4. Launch commands (A/B/C)

Common (from `hy_mt_runtime.rs` + `engine_launch.rs` on this 12-core host):

```
llama-server.exe -m <model> --alias HY-MT1.5-1.8B-Q4_K_M --host 127.0.0.1
  --port <port> -c 2048 -ngl <N> --jinja --no-webui --parallel 1 --threads 8
```

- Arm A (CPU baseline, as shipped today): `-ngl 0`
- Arm B (GPU): `-ngl 99`  (all 33 layers offloaded; verified)
- Arm B-nokv (GPU, KV on CPU): `-ngl 99 --no-kv-offload`
- Arm C (partial): `-ngl 32` (output layer + 1 transformer layer on CPU)

Client wire (production, from `foundry_local.rs` + `prompt_router.rs`):
`POST /v1/chat/completions`, single user message built by the prompt router
(en-US -> zh-CN, "将以下文本翻译为中文，注意只需要输出翻译后的结果，不要额外解释：\n\n<src>"),
`temperature 0.7, top_k 20, top_p 0.6, repeat_penalty 1.05, max_tokens 120`,
client timeout 30 s.

## Key interim finding (before the A baseline)

Adreno OpenCL **KV-cache offload is unreliable** in the packaged b10155 runtime:

- `-ngl 99` (KV offloaded): permanent inference hang after ~5.2 min / ~682
  requests under sequential load. Server HTTP loop stays responsive
  (/health 200 in 2 ms), the single slot stays `is_processing`, no OpenCL or
  driver error is logged, no TDR / driver-reset event in the Windows event
  log. The device-side compute path stalls silently and permanently.
- `-ngl 32` (KV still offloaded): survives 656 requests but shows clustered
  generation stalls of 2-24 s (9 stalls >2 s, max 23.4 s; tok/s floor 0.6).
- `-ngl 99 --no-kv-offload` (weights on GPU, KV on CPU): 656/656 requests
  clean, max 2.97 s, zero calls >3 s. GPU engine utilization 79.8% mean
  (attributed to the llama-server PID, engine type "3d"), engine process CPU
  1.5% mean.

The full-offload-with-KV hang and the partial-offload stalls both disappear
when KV stays on CPU; therefore KV offload is the trigger, not layer count.

## 5. Methodology

- Single fixed release `subtitle-eval` binary for all arms; only the server's
  `-ngl` differs. Same model file, same prompts, same input ordering
  (dataset order x runs), same context (`-c 2048`), same decoding defaults,
  same threads (8), same warm-up policy (harness's fixed zh->en sample before
  measured cases).
- Workload: real-session dataset `s3_2026-08-05_22-00-04.json` (656 en-US->
  zh-CN OCR-derived subtitle lines from a production session), repeated 3
  times sequentially = ~1,968 requests per arm. Includes short (<=20 ch),
  medium, and long (>=60 ch) lines. s3 is real-session evidence, not
  synthetic.
- Machine-readable counters sampled every ~1 s during each arm: GPU Engine
  utilization per LUID (with per-PID attribution), GPU Adapter shared/dedicated
  memory, GPU Process memory for the engine PID, engine process CPU%,
  working set / private bytes, system free RAM.
- Server per-request timings (tokens/s, eval ms) parsed from llama-server
  stderr log.
- Startup: process spawn -> /health ready (ms), model-load lines from server
  log, harness warm-up latency.
- Sustained stability: ~10-20 min continuous GPU run (see section 8).

## 6. Raw result artifact paths

All under the benchmark worktree, `eval-results/gpu-bench/` (gitignored):

- `datasets/` - real-session datasets + equivalence-100 subset
- `b-ngl99-<stamp>/` - Arm B run dir (server logs, gpu-counters.jsonl,
  eval-report.json, run-summary.json)
- `a-ngl0-<stamp>/` - Arm A run dir
- smoke runs: `b-ngl99-20260809-173229/173537/173831/`

## 7. Summary table

(pending battery completion - filled from analyze-gpu-bench.py)

## 8. Sustained-session stability

**Leading GPU config (`-ngl 99 --no-kv-offload`), 13m48s continuous, 1312
sequential requests:** no crash, no hang, no driver reset, no OpenCL error,
no allocation failure, no silent CPU fallback (GPU engine utilization stayed
79.3% mean / 83.6% p95 attributed to the llama-server PID, engine type "3d",
throughout; engine CPU 1.5% mean). Latency stayed flat across the session:
server-side p50 567 ms / p95 1072 ms / p99 1557 ms / max 3035 ms, one call
just over 3 s, zero over 10 s. Working set grew from ~3.0 GB to ~3.7 GB over
the session (model + KV + graph buffers; no unbounded growth), system RAM
free floor 5.3 GB of 32 GB.

In contrast, `-ngl 99` with KV cache offloaded hung permanently after ~5.2
min (see section 4 interim finding), and `-ngl 32` with KV offloaded produced
clustered 2-24 s generation stalls.

## 9. Output-equivalence sanity check

(pending - deterministic 100-case subset, seed pinned, A vs B)

## 10. GPU use verification

- OpenCL backend is compiled into the packaged runtime: `ggml-opencl.dll`
  present; `llama-bench --list-devices` enumerates platform `QUALCOMM
  Snapdragon(TM)` and device `Qualcomm(R) Adreno(TM) X1-85 GPU (OpenCL 3.0)`.
- `llama-server -lv 4` startup reports: `device GPUOpenCL (Qualcomm(R)
  Adreno(TM) X1-85 GPU) (16163 MiB, 15139 MiB free)`, `offloaded 33/33
  layers to GPU`, `OpenCL model buffer size = 1075.85 MiB`, kernel cache at
  `%LOCALAPPDATA%\llama.cpp\cl-cache`.
- During measured inference, Windows `GPU Engine` counters attribute 70-96%
  utilization of the Adreno "3d" engine to the llama-server PID, while the
  engine process CPU stays at a few percent. This is independent GPU-use
  verification (machine-readable counters, not Task Manager screenshots).
- No Khronos `OpenCL\Vendors` registry key exists on this machine; the
  Qualcomm OpenCL loader path is driver-shipped. Functional evidence above
  overrides registry-layout assumptions.
- `llama-server --list-devices` is a silent no-op on this build (known quirk);
  device enumeration verified via `llama-bench --list-devices` and `-lv 4`
  startup logs instead.

## 11. Recommendation

(pending)

## 12. Next step

(pending)
