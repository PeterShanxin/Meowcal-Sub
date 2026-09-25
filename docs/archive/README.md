# Archives

Work that is not on `main` but is kept for reference. Archive branches are on
`origin` and are never merged. Raw data from real episodes (OCR text,
translations, session datasets) is not public, so it is kept in the owner's
private Google Drive, in `My Drive/Meowcal-Sub-archive/`
(https://drive.google.com/drive/folders/1pw0dYRMjKqOd9g2sO_DAiZYzbVre1LHM).

| What | Branch | Private raw data |
| --- | --- | --- |
| Real-session model benchmark, HY-MT1.5 vs Hy-MT2 (August 2026) | `archive/benchmark-infra-2026-09-15`; see its `BENCHMARK_ARCHIVE.md` | `hy-mt2-benchmark-2026-08-08-private.zip`, SHA-256 `2fc22855f0fef4fad1e80266f61f754632f668c2bebdc620a3f508692164af93` |
| Adreno GPU offload benchmark (2026-08-09, #60) | `archive/gpu-adreno-arm64-bench-2026-08-09`; see its `GPU_BENCHMARK_ARCHIVE.md` | `gpu-adreno-bench-2026-08-09-raw.zip`, SHA-256 `b290758a4829ffa9746bf7401f71a45f7641a16e85a726539d6312b7a6c42d5b` |

Check a downloaded zip with `Get-FileHash <file>` before relying on it.

`plans/` holds an earlier design document that is no longer current.
