# Evidence: x64 Vulkan GPU policy for HY-MT

Date: 2026-11-07
Scope: decides when the x64 Vulkan runtime (`llama-b10155-vulkan-x64`) runs
HY-MT1.5-1.8B-Q4_K_M on the GPU, and what Core reports as `acceleration`.
Decision carried into this change: the x64 runtime uses its manifest policy
(`-ngl 99`) only when DXCore reports a discrete hardware adapter (hardware and
not integrated). Every other x64 host runs the CPU policy (`-ngl 0`) and
reports `acceleration: "cpu"`. A GPU start that fails readiness or the sample
translation retries on CPU, the same as the Adreno path.

## Machine and runtime

- Windows 11 Home build 26200 x64; Intel Core Ultra 7 255HX (20 cores, no SMT);
  31.3 GB RAM.
- Discrete: NVIDIA GeForce RTX 5060 Laptop GPU, 8 GB, driver 32.0.15.9649
  (NVIDIA 596.49), Vulkan API 1.4.329.
- Integrated: Intel(R) Graphics (Arrow Lake-HX), driver 32.0.101.8132,
  Vulkan API 1.4.325.
- Vulkan instance 1.4.321 (`vulkaninfo --summary`).
- Runtime and model are the shipped manifest artifacts; the archive,
  `llama-server.exe`, and model SHA-256 values matched the manifest.
- Launch line is the one Core builds on x64: `-c 2048 -ngl N --jinja
  --no-webui --parallel 1`, no `--threads`.

## GPU use

- `llama-server --list-devices` enumerates `Vulkan0` (RTX 5060) and `Vulkan1`
  (Intel Graphics).
- With both visible, `-lv 4` logs `using device Vulkan0 (NVIDIA GeForce RTX
  5060 Laptop GPU)` and `offloaded 33/33 layers to GPU`. llama.cpp selects the
  discrete device and ignores the integrated one.
- Windows `GPU Engine` counters for the llama-server PID during continuous
  translation: RTX 3D engine 85 % mean; with only the Intel device visible,
  Intel compute engine 98 % mean.

## Benchmark

Five English sentences written for this test, translated to Chinese,
`temperature 0`, `max_tokens 128`, via `/v1/chat/completions`. Three rounds;
the first round is warm-up. Medians over rounds 2-3 (10 requests). Device
selection used `GGML_VK_VISIBLE_DEVICES`.

| Configuration | Generation tok/s | Wall per request | Ready |
|---|---:|---:|---:|
| `-ngl 99`, both devices visible (shipped behavior) | 210 | 73 ms | 15.3 s first run, 1.7 s later |
| `-ngl 99`, RTX 5060 only | 221 | 68 ms | 1.7 s |
| `-ngl 99`, Intel Graphics only | 23 | 592 ms | 4.9 s |
| `-ngl 99`, no Vulkan device visible | 52 | 276 ms | 1.4 s |
| `-ngl 0` (CPU) | 50 | 267 ms | 1.4 s |

The first GPU start on a new driver spent about 14 s building Vulkan
pipelines; later starts used the driver cache. That fits inside the 45 s GPU
readiness window.

## Conclusions

- The discrete GPU is about 4x the CPU and is worth enabling.
- The integrated GPU on this host is about 2x slower than the CPU. Enabling the
  GPU whenever Vulkan works would make integrated-only hosts slower.
- With no Vulkan device, `-ngl 99` silently runs on the CPU. Core previously
  reported `acceleration: "gpu"` for every x64 launch with layers requested,
  which was wrong in that case and in the integrated-GPU case.

## Limitations

- One host, one driver per GPU. Stronger integrated GPUs (for example Arc
  140V) were not measured and stay on CPU until they are.
- A discrete GPU without a working Vulkan driver passes the DXCore check,
  starts with `-ngl 99`, and falls back to CPU inside llama.cpp. Core then
  reports `gpu` while the engine runs on the CPU. The sample check cannot
  detect this because the output is correct.
- No real subtitle media or OCR text was used; capture, OCR, and overlay were
  not exercised.
