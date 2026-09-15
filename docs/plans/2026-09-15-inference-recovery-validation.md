# Inference recovery validation — 2026-09-15

Tested source: `90d4f416f38e08f4c14bb8998371ec426d00a359`.
Windows 11 ARM64, build 26200, display scale 125%; Adreno X1-85,
driver 31.0.148.0. The Tauri development application used an isolated profile
and model cache, English OCR to Simplified Chinese, a 250 ms capture interval,
subtitle-only mode, and no translation context.

The app used its real Windows capture, OCR, HY-MT, and native overlay path.
The [offline fixture](../../evals/band-gate/README.md) ran at 1x with Repeat off.
GPU inference used the existing policy: `-ngl 99 --no-kv-offload`, eight threads,
one inference slot. No GPU or memory gate was overridden.

## Results

| Scenario | Observed result |
| --- | --- |
| Equal-width fixture, 120 seconds, after native region reselection | 30/30 cues recognized, admitted, and translated; gate and end-to-end reports passed without partial evidence; total translation latency p95 777.3 ms |
| Negative-band recovery, 120 seconds | All five recovery cues translated; timer and credits had zero post-warmup admissions; watermark had ten post-warmup admitted frames, so the negative gate failed |
| One controlled termination of the owned GPU engine | Recovery states appeared; exactly one replacement engine started; first successful translation returned 11,710 ms after termination; twelve translations succeeded after termination before stopping capture |
| Real CPU protocol checks | Six translations ended normally; translation continued after an intentionally expired request; quality review, duplicate-report handling, and Core shutdown succeeded |

![Authored subtitle and the native translated overlay](../assets/inference-recovery-native.png)

The first equal-width run produced thirty translations but captured its first
cue before the fixture clock started. Its report correctly accepted only 29/30.
The complete rerun above started the fixture before capture and passed.

The negative run lost OCR for three frames at 36.474–36.995 seconds. At
37.499 seconds the unchanged watermark became cue 2 and was admitted again.
The band-filter and cue-tracker sources are identical to the PR base; this is
an additional observed limitation, outside the inference-recovery change.
Zero timer/credits admissions and five successful dialogue recoveries do not
turn that run into a passing negative gate.

No natural GPU numerical corruption occurred during these runs. They therefore
do not prove recovery from an actual Adreno numerical fault or prevention of
the #215 bugcheck. Controlled Core tests cover health-green corrupt output,
bounded sampling, CPU fallback, duplicate reports, and recovery timeout.
Physical x64, long real-media playback, and packaged release validation remain
outside this evidence. Shipping requires a new Core release and consumer pins.

## Automated checks and artifact identity

`scripts/verify.ps1 -CoreSourceCandidate` passed. Final application edits also
passed fresh clippy with warnings denied and all 499 application unit tests.
The full gate included 81 Core unit tests, 14 Core service contracts, 16 app IPC
tests, two command contracts, 421 frontend tests, and five browser tests.
Dependency audit reported no vulnerabilities.

SHA-256 of the native test binaries:

- Application: `6ab36dd14cad5346e11881ac6d8546e2b5e6f772d7b0c38a2451cfec3929f82e`
- Core: `0ba953a13929f5b8d650a8dac6f03e50afcc06a5f51e15db5f91e9be5239bddd`

Content-aware gate frames, fixture clocks, translation events, and generated
reports remain local. Only the authored fixture image and aggregate findings
are published here.

## Recovery boundary verification

Source `297069225dbdfc5a0d9d6f986babe90b019a195c` tightens sample word
matching, adds kana/Hangul repetition detection, preserves strong corruption
reasons ahead of length rejection, and retains the producing engine's GPU
policy after process exit. Each reported defect reproduced in a failing test
before correction. The full source-candidate gate then passed with 85 Core and
500 application unit tests, plus the existing integration and frontend gates.

On the same ARM64 host, a real GPU completion supplied a receipt. The controlled
test terminated that owned GPU process and submitted a deliberate quality
report for the receipt. Core emitted `inferenceCpuLocked`, loaded and validated
CPU in 3,245 ms, and translated the next cue successfully in 197 ms. A further
`ready` call retained CPU policy; shutdown completed in 325 ms with exit code 0.
The tested working diff was matched to the source commit above by SHA-256.
Core binary SHA-256: `cba4b75c2a2355d21aedf58588e9da4dfcaed4fcf206ef19179815f069da086f`.

The rebuilt application completed the final native capture/OCR/overlay run on
September 16 using the same 120-second equal-width fixture, at 1x with Repeat
off and no pauses. All 30 cues were recognized, admitted, and translated;
the gate and end-to-end reports passed with no partial evidence or missed cues.
Total translation latency p95 was 410.75 ms. The default memory-headroom guard
selected CPU because available memory or commit was below 4 GiB; no guard was
overridden. This is a final-source CPU UI pass, alongside the separate real
GPU-to-CPU recovery check above.

Application binary SHA-256:
`3a6ad60e43a1e735d6c86bf28f2b7b59f716bf0a7adbe53ca027ba78e87e8d7a`.

![Final-source CPU translation in the native overlay](../assets/inference-recovery-final-native.png)
