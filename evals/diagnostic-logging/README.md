# Detailed logging benchmark

Run from the repository root on Windows with the repository Rust toolchain
and the application dependencies in the Cargo cache:

```powershell
.\evals\diagnostic-logging\run.ps1
.\evals\diagnostic-logging\run.ps1 -Runs 8 -RetainedRecords 1200
```

The harness runs the current checkout's encoder and diagnostics writer against
two synthetic 1200 × 120 buffers per sample, representing capture and OCR
evidence. It reports p50, p95 and maximum for encoding, storage and their
combined cost. The populated case starts with 1,200 small synthetic records by
default, below the eviction threshold. The empty case starts fresh and
accumulates records from the run. Both cases use temporary directories; the
installed app and its logs are not opened.

This measures the synchronous cost of the writes that the capture pipeline
awaits in Verbose mode. Basic mode does not run this image-writing path. The
harness does not run the app or measure frame cadence, OCR, cue retention or
translation. Use the [native playback comparison](../README.md#choose-a-scenario)
to measure the user's actual subtitle experience.
