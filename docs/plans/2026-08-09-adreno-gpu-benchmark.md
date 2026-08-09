# Evidence: Adreno GPU offload for ARM64 HY-MT (issue #60)

Date: 2026-08-09
Scope: decides the ARM64 manifest runtime configuration for HY-MT1.5-1.8B-Q4_K_M.
Decision carried into this change: `gpuLayers: 99` + `--no-kv-offload` for the
aarch64 runtime. Full benchmark detail and raw artifacts live in the benchmark
worktree (`bench/gpu-adreno-arm64`, `eval-results/gpu-bench/`, gitignored).

## Critical invariant (do not substitute)

**On the tested Windows ARM64 Adreno path, full layer GPU offload must be
paired with `--no-kv-offload`; plain `-ngl 99` is known to hang and must not
be substituted.**

## Machine and runtime

- HONOR MRO-XXX, Windows 11 Pro build 26200 ARM64, Snapdragon X Elite
  X1E80100, 12 cores, 32 GB RAM; Qualcomm Adreno X1-85 (driver 31.0.148.0,
  OpenCL 3.0 QUALCOMM build 863.0).
- llama.cpp b10155 release `llama-b10155-bin-win-opencl-adreno-arm64`
  (the shipped runtime; no runtime change was needed). Model
  HY-MT1.5-1.8B-Q4_K_M (33 offloadable layers, arch hunyuan-dense).
- GPU use independently verified: `llama-bench --list-devices` enumerates the
  Adreno device; `llama-server -lv 4` logs `offloaded 33/33 layers to GPU`;
  Windows `GPU Engine` counters attribute 79-96 % of the Adreno "3d" engine
  to the llama-server PID during inference while engine CPU stays ~1.5 %.

## Benchmark (same model/prompt/decoding; only -ngl differs)

Workload: 656-line real-session OCR dataset, sequential single-slot requests.

| Metric | A CPU `-ngl 0` | GPU `-ngl 99` | GPU `-ngl 99 --no-kv-offload` |
| --- | ---: | ---: | ---: |
| p50 / p95 / p99 (client) | 483 / 1474 / 3637 ms | 496* / 915* / 1364* ms | 576 / 1085 / 1561 ms |
| max / calls >3 s / >10 s | 16.9 s / 24 / 2 | 3.2 s* / 1* / 0* | 3.0 s / 1 / 0 |
| engine CPU | ~3.6 cores | ~0.2 | ~0.2 |
| sustained session | - | HANG at ~5.2 min | 13m48s / 1312 req, no hang |

\* partial run before the hang. `-ngl 32` with KV offloaded also produced
multi-second stalls (max 23.9 s).

## Failure attribution

The instability is associated with the **KV-cache GPU offload path on the
tested llama.cpp b10155 + Qualcomm Adreno/OpenCL driver combination**. We have
not proven whether the underlying defect belongs to llama.cpp, Qualcomm's
driver, or their interaction. The hang is silent: HTTP stays responsive, the
inference slot stays busy, no OpenCL/driver error is logged, no TDR/WHEA event
appears.

## Output equivalence (deterministic, seed 2026, 100 cases)

- Same-backend control (CPU run vs CPU run): 100/100 exact.
- CPU vs GPU-nokv: 25/100 exact; 75 diverged (backend numerical variation).
  Targeted review of all 24 low-similarity pairs plus a deterministic 20-case
  sample: 31 equivalent, 5 CPU-better, 2 GPU-better, 3 both-bad (OCR noise),
  1 GPU malformed artifact, 2 uncertain. No systematic semantic regression,
  no language drift, no truncation, no corruption cascade.

## Product trade-off

GPU-nokv is ~90-100 ms slower at median but materially improves the tail that
caused #60's outages: p99 3.64 s -> 1.56 s, max 16.9 s -> 3.0 s, >10 s 2 -> 0,
and frees ~3.4 CPU cores for OCR/capture/overlay contention. Startup adds
~3-6 s (GPU device init + model upload).
