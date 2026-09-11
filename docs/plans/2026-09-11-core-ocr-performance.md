# Meowcal Core OCR performance evidence

This record captures the local ARM64 comparison used during the Core extraction.
Sub 1 passed all 15 fixture/policy cases. Sub 2 failed the saturation run because
the colored fixture's candidate P95 exceeded its budget by 1.2219 ms; the separate
4 Hz run passed all five fixtures. Recognition results were equivalent in every
reported comparison.

The method and acceptance limits are maintained in
[Core OCR performance verification](../CORE_PERFORMANCE.md). Sub 1 used the
[binary comparison driver](../../scripts/benchmark_core_ocr.py); Sub 2 used its
corresponding `scripts/benchmark_core_ocr.py` driver. The retained report files
are `core/target/ocr-sub1-binary-reuse/report.json`,
`output/ocr-binary-reuse/report.json`, and
`output/ocr-binary-reuse-paced/report.json` in their respective repositories.

## Test identity and scope

- Host: Windows build 26200, ARM64, Snapdragon X Elite.
- Order: one ABBA cycle, baseline/candidate/candidate/baseline.
- Sampling: 100 measured calls and five warmups per block, giving 200 measured
  calls per variant for each case.
- Sub 1 baseline commit: `822ce922ae74dca16b216f8fe3ed6de228de07cf`.
  The report identifies the baseline and candidate benchmark executables by
  SHA-256 `c8c1acf29c3b03e87bc6d33fc41a40f45dbe9a3755b619e79f6d1f891f6946c7`
  and `a0f67ce757eb0d2774a8e1288ee0c21b12919ba6f38c5bca1132493e4b9a5637`.
  The candidate was built from the integration working tree, so this record does
  not assign it to a final commit.
- Sub 2 baseline commit and candidate checkout HEAD:
  `a2d13f97254f7489c214154d96d18472affd798a` and
  `ffc708c7e2123411fd45f8edd85459956f3155b7`.
  The candidate included the uncommitted binary transport changes; its HEAD
  alone does not reproduce the measured source.
- Both reports identify the exercised Core executable by SHA-256
  `3b3245e4e8105212766ba3e2ecf8fb0bfa05c094daf9b9daeac77370e1c227e9`.

These timings include each client's preprocessing, OCR calls, transport, and
result selection. They exclude capture, translation, UI scheduling, and native
presentation.

## Sub 1 steady-state results

`B -> C` means baseline to candidate. Recognition is `equivalent` only when all
warmup and measured text, line, rectangle, and frame-width results matched and
the nonblank/blank content check passed.

| Case | P50 ms B -> C | P95 ms B -> C | >250 ms B -> C | Recognition | Budget |
|---|---:|---:|---:|---|---|
| latin/single | 6.270 -> 6.692 | 7.966 -> 10.417 | 0.0% -> 0.0% | equivalent | pass |
| latin/raw | 4.877 -> 5.896 | 14.463 -> 7.905 | 0.0% -> 0.0% | equivalent | pass |
| latin/multi3 | 25.719 -> 21.169 | 73.221 -> 29.501 | 0.0% -> 0.0% | equivalent | pass |
| large/single | 17.608 -> 22.288 | 21.653 -> 30.763 | 0.0% -> 0.0% | equivalent | pass |
| large/raw | 13.194 -> 15.658 | 17.791 -> 20.377 | 0.0% -> 0.0% | equivalent | pass |
| large/multi3 | 51.825 -> 58.319 | 68.919 -> 76.605 | 0.0% -> 0.0% | equivalent | pass |
| color/single | 6.005 -> 7.195 | 7.193 -> 10.377 | 0.0% -> 0.0% | equivalent | pass |
| color/raw | 4.783 -> 6.129 | 6.902 -> 9.306 | 0.0% -> 0.0% | equivalent | pass |
| color/multi3 | 18.750 -> 19.847 | 23.902 -> 23.737 | 0.0% -> 0.0% | equivalent | pass |
| chinese/single | 6.114 -> 7.523 | 9.679 -> 11.522 | 0.0% -> 0.0% | equivalent | pass |
| chinese/raw | 5.303 -> 5.855 | 7.511 -> 7.330 | 0.0% -> 0.0% | equivalent | pass |
| chinese/multi3 | 17.460 -> 20.236 | 21.375 -> 25.653 | 0.0% -> 0.0% | equivalent | pass |
| blank/single | 3.189 -> 3.637 | 3.798 -> 4.444 | 0.0% -> 0.0% | equivalent | pass |
| blank/raw | 2.007 -> 2.818 | 2.506 -> 3.249 | 0.0% -> 0.0% | equivalent | pass |
| blank/multi3 | 9.214 -> 11.054 | 10.480 -> 11.970 | 0.0% -> 0.0% | equivalent | pass |

## Sub 1 cold initialization

Cold initialization and first-call latency are kept separate from the steady-state
percentiles. Each cell contains the two process observations for that variant,
in milliseconds.

| Case | Baseline init | Candidate init | Baseline first call | Candidate first call |
|---|---:|---:|---:|---:|
| latin/single | 11.081 / 5.885 | 119.441 / 58.457 | 36.872 / 15.542 | 15.260 / 16.123 |
| latin/raw | 5.133 / 5.467 | 79.859 / 66.601 | 15.123 / 13.373 | 17.322 / 15.286 |
| latin/multi3 | 24.563 / 5.990 | 82.036 / 79.794 | 79.783 / 29.575 | 35.752 / 40.269 |
| large/single | 6.217 / 5.141 | 87.986 / 128.177 | 31.860 / 26.445 | 32.408 / 75.245 |
| large/raw | 6.818 / 6.383 | 71.786 / 66.347 | 25.094 / 83.643 | 24.581 / 24.360 |
| large/multi3 | 9.947 / 6.552 | 65.037 / 82.224 | 70.485 / 66.468 | 64.967 / 74.784 |
| color/single | 5.216 / 5.346 | 57.901 / 79.243 | 15.320 / 16.059 | 17.265 / 15.220 |
| color/raw | 6.541 / 4.776 | 60.541 / 57.308 | 16.924 / 19.667 | 15.481 / 16.441 |
| color/multi3 | 10.633 / 5.848 | 63.071 / 61.240 | 32.229 / 26.140 | 31.359 / 30.030 |
| chinese/single | 8.240 / 5.242 | 80.223 / 94.599 | 43.024 / 37.654 | 59.009 / 49.060 |
| chinese/raw | 4.553 / 5.171 | 60.994 / 68.169 | 33.478 / 33.801 | 37.299 / 35.524 |
| chinese/multi3 | 5.841 / 7.584 | 63.060 / 57.614 | 43.731 / 54.934 | 61.065 / 51.552 |
| blank/single | 6.604 / 4.941 | 66.366 / 63.319 | 11.479 / 10.643 | 12.900 / 12.797 |
| blank/raw | 4.878 / 5.535 | 59.317 / 57.306 | 8.170 / 10.074 | 10.579 / 10.735 |
| blank/multi3 | 4.694 / 5.316 | 57.850 / 58.974 | 16.377 / 17.599 | 19.184 / 18.514 |

Sub 2 retains per-process first-call samples, but its reports do not expose a
separate Core initialization timer. No Sub 2 cold-initialization result is
claimed here.

## Sub 2 saturation results

The saturation run used no pacing between calls. All recognition comparisons
were equivalent and meaningful. The colored fixture still failed the latency
budget: candidate P95 was 39.4043 ms against a 23.1824 ms baseline and a
38.1824 ms limit, 1.2219 ms over budget. This makes the full saturation report
a failure.

| Case | P50 ms B -> C | P95 ms B -> C | >250 ms B -> C | Recognition | Budget |
|---|---:|---:|---:|---|---|
| latin | 22.050 -> 20.314 | 25.767 -> 24.941 | 0.0% -> 0.0% | equivalent | pass |
| large | 80.512 -> 76.525 | 102.254 -> 98.010 | 0.0% -> 0.0% | equivalent | pass |
| color | 18.641 -> 18.170 | 23.182 -> 39.404 | 0.0% -> 0.0% | equivalent | **fail** |
| chinese | 96.413 -> 27.544 | 129.879 -> 80.893 | 0.0% -> 0.0% | equivalent | pass |
| blank | 11.500 -> 13.807 | 15.242 -> 18.751 | 0.0% -> 0.0% | equivalent | pass |

## Sub 2 4 Hz results

The paced run used a 250 ms interval and passed all five comparisons.

| Case | P50 ms B -> C | P95 ms B -> C | >250 ms B -> C | Recognition | Budget |
|---|---:|---:|---:|---|---|
| latin | 37.010 -> 30.591 | 60.408 -> 59.066 | 0.0% -> 0.0% | equivalent | pass |
| large | 97.112 -> 88.830 | 136.304 -> 116.291 | 0.0% -> 0.0% | equivalent | pass |
| color | 30.039 -> 26.977 | 42.414 -> 39.553 | 0.0% -> 0.0% | equivalent | pass |
| chinese | 100.543 -> 30.339 | 135.707 -> 53.102 | 0.5% -> 0.0% | equivalent | pass |
| blank | 17.602 -> 17.984 | 35.912 -> 24.870 | 0.0% -> 0.0% | equivalent | pass |

## Limits of this evidence

The Sub 2 raw records include `clientCpuSeconds`, which measures only the Python
client process. It is not total CPU usage and is not used for a CPU conclusion.
These synthetic ARM64 results do not verify physical x64 performance, native
playback, screen capture, selector behavior, overlay presentation, or full-episode
user experience. They identify the tested binaries and source revisions, but do
not certify a final merged commit or replace the native release gates.
