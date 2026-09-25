# Adreno GPU benchmark archive (2026-08-09)

This branch preserves the tooling and the full write-up from the Adreno GPU
offload benchmark for HY-MT1.5-1.8B-Q4_K_M on Windows ARM64 (#60). The
condensed evidence that `main` relies on is
`docs/plans/2026-08-09-adreno-gpu-benchmark.md` on `main`. The version of
that file on this branch is the full report.

## What is here

- `docs/plans/2026-08-09-adreno-gpu-benchmark.md`: full method, per-configuration
  results (`-ngl 0`, `-ngl 32`, `-ngl 99` with and without KV offload),
  sustained-session stability, and the recommendation.
- `docs/plans/2026-08-09-adreno-gpu-draft.patch`: the draft production change
  the benchmark proposed. It was not merged as written.
- `scripts/run-gpu-bench.ps1`: drives the controlled A/B (and optional C) runs
  through the production eval harness, one `-ngl` configuration per arm.
- `scripts/sample-gpu-counters.ps1`: samples GPU, CPU, and RAM about once a
  second into JSONL during a run.
- `scripts/analyze-gpu-bench.py`: merges each arm's run summary and counter
  samples into one comparison table.
- `scripts/build-equivalence-subset.py`, `scripts/compare-equivalence.py`,
  `scripts/dump-divergence.py`: build the 100-line equivalence set and compare
  CPU and GPU output line by line.
- `scripts/analyze-app-log.py`: extracts per-frame model timings from an app
  session log for gate evidence.

## Raw results

The raw run directories (`eval-results/gpu-bench/`, 78 files) are not in this
repository. They contain OCR text and translations from real episodes, which
must not be published.

They are stored privately in the owner's Google Drive:

- Folder: `My Drive/Meowcal-Sub-archive/`
  (https://drive.google.com/drive/folders/1pw0dYRMjKqOd9g2sO_DAiZYzbVre1LHM)
- File: `gpu-adreno-bench-2026-08-09-raw.zip`
  (https://drive.google.com/file/d/1QbzLO8yP-BzKmwvWOMB9UzA3OcUZcK1p/view)
- Size: 2,592,712 bytes
- SHA-256: `b290758a4829ffa9746bf7401f71a45f7641a16e85a726539d6312b7a6c42d5b`

Unzipping it gives `gpu-bench/<run-id>/`, which is the layout the scripts
expect under `eval-results/`.
