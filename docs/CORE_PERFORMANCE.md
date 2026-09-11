# Core OCR performance verification

Core's process boundary is accepted only with comparable latency and recognition
evidence. [ADR-0004](adr/0004-versioned-meowcal-core.md) defines the budgets.
The benchmark excludes screen acquisition, translation and native presentation;
it includes each product's preprocessing, recognition passes, transport and
result selection. It cannot establish full-episode UX by itself.

## Repeatable comparison

Use identical Windows hardware, installed OCR languages and synthetic fixtures
for both versions. Stop task-owned builds before measuring; retain slow samples.
Record the baseline commit, candidate commit and executable SHA-256 values.
Run release builds in baseline/candidate/candidate/baseline order. Warmup is
explicit and first-call/initialization timings remain separate from steady state.

Sub 2's `scripts/benchmark_core_ocr.py` generates Latin, Chinese, colored, blank
and large-region fixtures, including packed BGRA files and their checksums. Its
`--baseline`, `--candidate`, `--core` and `--output` arguments select independent
checkouts and the built Core. The baseline needs its original `winocr` dependency.
Each worker verifies the imported package path, so an editable Python install
cannot silently substitute the other checkout. A nonzero exit reports a budget
or recognition mismatch; raw samples are retained in the output directory.
Run the default saturation test and a separate `--interval-ms 250` comparison
for the normal four-per-second cadence. Keep both reports: pacing must not hide
a regression under sustained work.

For Sub 1, build `src-tauri/examples/ocr_benchmark.rs` with
`cargo build --manifest-path src-tauri/Cargo.toml --release --example ocr_benchmark`.
Build the same example in a separate pre-Core checkout, omitting only the two
Core registration and shutdown statements. Keep all recognition calls and
measurement code identical, and use the same release profile. Validation-only
Tauri resources can be prepared with `scripts/prepare-validation-resources.ps1`;
these placeholders are not an application package.

Set `MEOWCAL_CORE_EXECUTABLE` to the candidate Core's absolute path and invoke:

```powershell
python scripts/benchmark_core_ocr.py `
  --baseline <baseline-example.exe> --candidate <candidate-example.exe> `
  --fixtures <fixtures-directory>/fixtures.json --output <new-output-directory> `
  --runs 100 --warmup 5
```

The Sub 1 driver exercises default preprocessing, raw recognition and three-pass
selection. It compares text, lines, rectangles and frame width, including warmup
results. Both drivers retain P50, P95, maximum and the proportion exceeding
250 ms. Empty output is permitted only for the blank fixture. A successful run
establishes parity on that fixture matrix and host, not OCR accuracy for all
subtitle styles or another hardware architecture.

## Release evidence

The native gate additionally covers capture scaling, changing/blank/repeated
frames, language changes, cancellation and recovery, and the selector/overlay
during real playback. Record architecture and Windows build for each run.
Windows x64 emulation or hosted ARM64 CI does not establish physical x64 or
Snapdragon performance respectively. Keep unresolved budget failures and manual
gates visible in the Draft PR until directly verified.
