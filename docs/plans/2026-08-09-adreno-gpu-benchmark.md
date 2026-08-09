# Adreno GPU Offload Benchmark: HY-MT1.5-1.8B-Q4_K_M on Windows ARM64

Status: COMPLETE (benchmark, evidence, recommendation - no production default
was changed, no merge was made)
Date: 2026-08-09
Scope: controlled A/B/C benchmark of Adreno OpenCL GPU offload vs the current
CPU-only ARM64 configuration, for issue #60. NPU/QNN/ONNX explicitly out of
scope. The HY-MT model/quantization/prompt/decoding were not changed.

## 1. Main SHA tested

- `main` HEAD at benchmark start: `856d8648e7d5e8e3faae298a2f559a977c03eef7`
- Worktree base: `integrate/benchmark-infra` @ `ee24711` (unmerged infra branch)
- Benchmark branch: `bench/gpu-adreno-arm64` @ `6ba1bd2` + later commits
  (instrumentation + docs only; NOT for merge)

## 2. Branch / worktree

- Worktree: `C:\FormerD\Repos\.wt-meowcal-gpu-bench`
- Branch: `bench/gpu-adreno-arm64`
- Unrelated worktrees (`main`, `.wt-meowcal-mt2`, `.wt-meowcal-bench-integ`)
  and all uncommitted work elsewhere were left untouched.

## 3. Exact runtime / version

- Model: `HY-MT1.5-1.8B-Q4_K_M.gguf`, sha256 `4383ac0c...c477c7` (matches
  manifest), 1,133,080,512 bytes. Path
  `C:\FormerD\foundry-cache\meowcal-sub\models\hy-mt1.5-1.8b-q4\`
- Runtime: llama.cpp **b10155** release
  `llama-b10155-bin-win-opencl-adreno-arm64.zip` (manifest sha256
  `1b0ead2e...9b07`); `llama-server.exe` sha256 `8cbee8b9...b8d14` (matches
  manifest). Model arch `hunyuan-dense`, 32 transformer layers + output layer
  (33 offloadable), n_ctx_train 262144, n_embd 2048.
- Eval binary: single `subtitle-eval.exe` release build, identical for all arms.
- Host: HONOR MRO-XXX, Windows 11 Pro build 26200 ARM64, Snapdragon X Elite
  X1E80100 (12 cores / 12 threads), 32 GB RAM (4.9-6.5 GB free under ambient
  OneDrive/WorkBuddy/chrome load - the machine was NOT quiesced, matching the
  #60 operating condition).
- GPU: Qualcomm Adreno X1-85, driver 31.0.148.0. OpenCL 3.0 QUALCOMM build
  863.0 Compiler DX.50.39.00; device global mem 16,163 MiB; max single
  allocation 2048 MiB; no SVM. LUID `0x00000000_0x00010d66`.
- Power scheme: Balanced (381b4222-f694-41f0-9685-ff5bb260df2e), unchanged.

## 4. Launch commands (A/B/C)

All arms: `llama-server.exe -m <model> --alias HY-MT1.5-1.8B-Q4_K_M
--host 127.0.0.1 --port <p> -c 2048 --jinja --no-webui --parallel 1
--threads 8` (identical to the app's launch path from `hy_mt_runtime.rs` +
`engine_launch.rs`; threads 8 = worker_threads(12) on this host), plus:

- **Arm A (CPU, as shipped)**: `-ngl 0`
- **Arm B (GPU full)**: `-ngl 99`
- **Arm B-nokv (GPU, KV on CPU)**: `-ngl 99 --no-kv-offload`
- **Arm C (GPU partial)**: `-ngl 32`

Client wire (production, from `foundry_local.rs` + `prompt_router.rs`):
`POST /v1/chat/completions`, single user message
"将以下文本翻译为中文，注意只需要输出翻译后的结果，不要额外解释：\n\n<src>"
(en-US -> zh-CN), temperature 0.7 / top_k 20 / top_p 0.6 /
repeat_penalty 1.05 / max_tokens 120, client timeout 30 s. `--parallel 1`,
single slot, one request at a time (production behavior).

## 5. Benchmark methodology

- Same model file, same prompt router, same input dataset and ordering
  (dataset order x runs), same `-c 2048`, same decoding defaults, same
  `--threads 8`, same warm-up policy (harness's fixed zh->en sample before
  measured cases), same release `subtitle-eval` binary, same machine/power
  mode. Only the server's offload configuration differed.
- Workload: real-session dataset `s3_2026-08-05_22-00-04.json` - 656 OCR-
  derived en-US->zh-CN subtitle lines from a production viewing session
  (real-session evidence, not synthetic), covering short (<=20 ch), medium and
  long (>=60 ch) lines; repeated sequentially (`--runs 3` = 1,968 requests for
  A; `--runs 2` = 1,312 for the sustained B-nokv run).
- Machine-readable counters sampled every ~1-4 s per arm: Windows `GPU Engine`
  per-LUID utilization with per-PID attribution, `GPU Adapter Memory`
  shared/dedicated, `GPU Process Memory` for the engine PID, engine process
  CPU% and working set, system free RAM.
- Server-side per-request prompt-eval/generation timings and tokens/s parsed
  from llama-server stderr.
- Startup: process spawn -> /health ready; harness warm-up latency.
- Sustained stability: 13m48s continuous single-session run (1,312 requests)
  on the leading GPU config.

## 6. Raw result artifact paths

All under the benchmark worktree `eval-results/gpu-bench/` (gitignored):

- `datasets/` - real-session datasets; `equivalence-100.json` deterministic subset
- `a-ngl0-20260809-182033/` - Arm A battery (1,968 req)
- `a-ngl0-20260809-184158/` + `a-ngl0-seedcontrol/` - equivalence runs (seed 2026)
- `b-ngl99-20260809-174003/` - Arm B battery (HUNG at ~682 req; partial data)
- `b-ngl99-20260809-180626/` - Arm B-nokv sustained (1,312 req, 13m48s)
- `b-ngl99-20260809-175756/` - Arm B-nokv probe (656 req)
- `c-ngl32-20260809-174659/` - Arm C partial (656 req)
- `b-ngl99-20260809-173831/` (+ 173229/173537) - smoke runs
- Each dir: `server.log`/`server.err.log`, `gpu-counters.jsonl`,
  `eval-report.json`, `run-summary.json` (launch args + hashes + timing).
- Report/doc: `docs/plans/2026-08-09-adreno-gpu-benchmark.md`,
  draft patch: `docs/plans/2026-08-09-adreno-gpu-draft.patch`.

## 7. Summary table

Client-side request latency from the eval harness; tok/s and util from server
log / counters. B's row is server-side numbers for the 682 completed requests
before its hang (no client report was produced).

| Metric | A CPU (1968) | B GPU (682, then HANG) | C ngl32 (656) | B-nokv GPU (1312) |
| --- | ---: | ---: | ---: | ---: |
| p50 latency | 483 ms | 496 ms* | 621 ms | 576 ms |
| p95 latency | 1,474 ms | 915 ms* | 1,372 ms | 1,085 ms |
| p99 latency | 3,637 ms | 1,364 ms* | 2,490 ms | 1,561 ms |
| max latency | 16,863 ms | 3,221 ms* | 23,950 ms | 3,037 ms |
| gen tok/s (server p50) | 35.2 | 30.8 | 27.3 | 24.8 |
| calls >3 s | 24 | 1* | 4 | 1 |
| calls >10 s | 2 | 0* | 2 | 0 |
| generation stalls >2 s | 43 | - | 9 | 2 |
| engine CPU (cores busy) | ~3.6 | ~0.2 | ~3.8 | ~0.2 |
| GPU util (llama PID, mean) | 0 % | ~80 % | ~80 % | 79.3 % |
| GPU engine type | - | 3d | 3d | 3d |
| process WS (mean) | 3.20 GB | 2.92 GB | 3.15 GB | 3.42 GB |
| GPU shared mem (max) | 1.31 GB* | 2.71 GB* | n/a* | 2.55 GB |
| RAM free (min) | 4.90 GB | 4.30 GB | 4.25 GB | 5.28 GB |
| startup (spawn -> health) | 1.6 s | 5.3-5.7 s | 4.7 s | 6.3-7.5 s |
| first-request warm-up | 109 ms | 233-269 ms | 236 ms | 288-342 ms |

\* B and A/C32 rows: sampler byte-field capture was partially lost to a PS 5.1
`ContainsKey` bug on some runs (utilization/CPU/WS unaffected); B row is
server-side, partial-run data. `GPU shared mem` on A is other processes'
usage (llama uses none on CPU).

Caveat: A and B-nokv batteries ran in adjacent time windows (18:06-18:20 and
18:20-18:41); ambient machine load drifted within both windows. The harness
is deterministic (same-backend seed control = 100 % exact), and the B-nokv
tail behavior was reproduced in a second independent run (probe, 656 req).
A's tail behavior matches the #60 session evidence (loaded CPU p90 3.07 s).

## 8. Sustained-session stability

**Leading GPU config (B-nokv), 13m48s continuous, 1,312 sequential
requests:** no crash, no hang, no driver reset, no OpenCL error, no
allocation failure, no silent CPU fallback. GPU engine utilization stayed
79.3 % mean / 83.6 % p95 attributed to the llama-server PID (engine "3d")
throughout; engine CPU 1.5 % mean. Latency stayed flat: server-side p50 567
ms / p95 1,072 ms / p99 1,557 ms / max 3,035 ms; one call just over 3 s, zero
over 10 s. Working set grew ~3.0 -> ~3.7 GB (bounded, model + KV + graph
buffers); system RAM free floor 5.3 GB of 32 GB.

**Arm B (plain -ngl 99, KV offloaded) is NOT stable:** after ~5.2 min / ~682
sequential requests the inference slot wedged permanently (HTTP /health still
200 in 2 ms; slot stuck `is_processing`; no OpenCL/driver error in the log;
no TDR/WHEA event in the Windows event log). The device-side compute path
stalls silently and never resumes.

**Arm C (-ngl 32, KV offloaded) survives but stalls:** 656/656 completed,
yet clustered generation stalls of 2-24 s (max 23.4 s; tok/s floor 0.6) - the
same underlying Adreno KV-offload instability in shorter-lived form.

**Root-cause attribution:** the failure is the KV-cache-offload path of the
llama.cpp b10155 OpenCL backend on the Adreno driver (OpenCL 3.0 QUALCOMM
build 863.0). Layer count is not the trigger: B-nokv keeps all 33 layers on
the GPU and is stable when KV stays on CPU. No separate llama.cpp build or
runtime download was needed - the shipped b10155 OpenCL runtime does provide
working GPU acceleration once KV offload is disabled. Temperature was not
measured (no non-invasive Windows SoC temp source); no throttling signature
observed in the timing data.

## 9. Output-equivalence sanity check

Deterministic 100-case subset (stride-selected from s3, src len 4-76),
seed 2026 pinned server-side, runs on A and B-nokv:

- Same-backend control: A run1 vs A run2 = **100/100 exact** (the harness is
  bit-reproducible; any A-vs-GPU divergence is backend, not run noise).
- A vs B-nokv: **25/100 exact**; remaining 75 have similarity median 0.65
  (min 0.00 / max 0.944); 24 cases below 0.5.
- Reviewed the 12 lowest-similarity pairs with source: the divergence is
  backend numerical variation producing **different-but-mostly-valid Chinese
  renderings** (e.g. "我什么都不相信。" vs "我不相信任何东西。"). No language
  drift, no truncation, no repetition loops in this set. Isolated artifacts
  on noisy OCR lines on BOTH arms: A produced 2 garbage passthrough outputs
  ("oaes", "swaD"); B produced 1 doubled-quote artifact. Grader rejections:
  A 5/100, B-nokv 3/100 (all source_passthrough).
- Big-battery grader failure rates were comparable: A 50/1,968 (2.5 %) vs
  B-nokv 24/1,312 (1.8 %), same classes (source_passthrough dominates on both).

Conclusion: a small backend-dependent numerical divergence exists and is
acceptable - translation behavior remains semantically normal; no
unexplained material divergence was found.

## 10. GPU use independently verified

Yes, via three independent machine-readable sources (not archive filename,
not Task Manager):

1. `llama-bench --list-devices` on the packaged runtime enumerates platform
   `QUALCOMM Snapdragon(TM)`, device `Qualcomm(R) Adreno(TM) X1-85 GPU
   (OpenCL 3.0)`.
2. `llama-server -lv 4` startup: `device GPUOpenCL (Qualcomm(R) Adreno(TM)
   X1-85 GPU)`, `offloaded 33/33 layers to GPU`, `OpenCL model buffer size =
   1075.85 MiB`.
3. During measured inference, Windows `GPU Engine` counters attribute
   79-96 % utilization of the Adreno "3d" engine to the llama-server PID
   (e.g. 79.3 % mean / 83.6 % p95 over the sustained run), while the engine
   process uses ~1.5 % CPU. On the CPU arm the llama PID's GPU utilization is
   0 %. The OpenCL kernel cache (`%LOCALAPPDATA%\llama.cpp\cl-cache`) was
   already warm (7/28), so no first-run compile cost is hidden in the data.

Note: `llama-server --list-devices` is a silent no-op in this build (quirk);
device enumeration was verified via `llama-bench` and the `-lv 4` startup
log. No Khronos `OpenCL\Vendors` registry key exists on this machine - the
Qualcomm OpenCL loader is driver-shipped; functional evidence above
overrides registry-layout assumptions.

## 11. Recommendation

**PROMISING BUT NEEDS MORE WORK.**

GPU offload is NOT "clearly faster": on this workload the Adreno path is
~90-100 ms slower at p50 (483 vs 576 ms) and slower per token (prefill 12.1
vs 24.9 ms/token; generation 35.2 vs 24.8 tok/s) - a 1.8B model on a strong
12-core Oryon CPU is hard to beat on raw speed.

GPU offload IS "materially more robust under load", which is what live
subtitles need (per the task's own success criteria): p95 -389 ms, p99
-2,076 ms, max -13.8 s, calls >3 s 24 -> 1, calls >10 s 2 -> 0, generation
stalls >2 s 43 -> 2, and engine CPU freed from ~3.6 cores to ~0.2 cores
(removing the #60 root-cause-2 contention). The 13m48s sustained session was
clean. These are the exact properties that eliminate the 15-22 s subtitle
outages reported in #60.

Not READY FOR DEFAULT yet because:

1. The correct configuration is `-ngl 99 --no-kv-offload`, NOT plain
   `-ngl 99`: shipping `gpuLayers 99` alone (the natural change) would ship
   a ~5-minute-then-hang engine. The kv-offload constraint must be baked into
   the manifest design.
2. The p50 regression (~100 ms) and +3-6 s startup need acceptance by the
   product, and the engine-only benchmark cannot prove the end-to-end
   contention win; that needs the app-level gate (OCR + capture + WebView2
   running) on this machine.
3. Optimal layer count was not exhaustively swept; B-nokv (all layers) was
   chosen as the minimum meaningful configuration that fixes the failure,
   per the task's guidance to avoid parameter sweeps.

**DO NOT ENABLE** applies to plain `-ngl 99` (hang) and to `-ngl 32`-style
partial offload with KV on the GPU (multi-second stalls).

## 12. Exact proposed next step

1. Apply the draft patch `docs/plans/2026-08-09-adreno-gpu-draft.patch`
   (validated with `git apply --check` and `cargo check`): aarch64 manifest
   entry `acceleration: "gpu"`, `gpuLayers: 99`, per-runtime
   `launchArgs: ["--no-kv-offload"]` (new optional `RuntimeSpec.launch_args`
   field so the x64 Vulkan runtime is untouched).
2. Run the app-level manual gate on this machine: live OCR + capture +
   WebView2 contention with the patched manifest, at least one real episode
   (per `docs/AGENT_GUIDE.md` manual-gate rules), confirming the tail/outage
   behavior from #60 is gone and translations stay normal.
3. Only if that gate passes and the ~100 ms median cost is accepted: merge
   as a reviewed PR, ship in the next release.
4. Follow-ups (out of scope here): a second CPU battery for tail
   reproducibility under controlled ambient load; a decision on whether the
   p50 cost matters at the 250 ms capture cadence.
