# ADR-0007: Embedded PP-OCRv6 recognizer in Core

**Date:** 2026-11-06  
**Status:** Accepted  
**Decision owners:** Meowcal maintainers  
**Related:** [ADR-0001](0001-curated-local-translation-stack.md),
[ADR-0004](0004-versioned-meowcal-core.md), issue #59, pull request #298

## Context

ADR-0001 keeps Meowcal Sub on Windows OCR. Windows OCR reads an unchanged
subtitle differently from one frame to the next: it splits characters into
radicals and swaps look-alikes. Each different reading looks like a new cue and
starts a new translation, so the overlay text changes while the subtitle does not.

The comparison in
[`docs/plans/2026-11-05-ocr-stability-experiment.md`](../plans/2026-11-05-ocr-stability-experiment.md)
measured this on a real Chinese clip and on synthetic Chinese and English
corpora. Windows OCR, with and without the white-glyph mask, gave the exact text
on 29-70% of unchanged frames and 1.7-9.7 distinct readings per cue. PP-OCRv6
small gave 87-100% and 1.0-2.2. Preprocessing in the application cannot remove
the difference: the instability is in the recognizer.

## Decision

Core runs PP-OCRv6 small as a second OCR engine. API 1 gains an optional
`engine` parameter on `ocrInitialize` and `ocrRecognizeBgra` and the
`ocrPpOcrV6` capability. A request without `engine` uses Windows OCR, so Sub 2
and older Sub 1 releases keep their behavior.

- Inference uses RTen, a Rust ONNX runtime. Core keeps no native inference
  library to package, sign, or load from a search path.
- The detector and recognizer models are vendored in `core/models/ppocr` and
  compiled into the Core executable. The reviewed Core archive digest then
  authenticates them, and the archive layout that consumers verify does not
  change. A test pins both model digests.
- The `ppocr` Cargo feature, on by default, gates the runtime and the models.
  Consumers that link Core only for protocol types turn it off.
- Inference uses at most four CPU threads, because the translation engine runs
  on the same CPU.

Meowcal Sub will default to PP-OCRv6. Windows OCR will stay selectable, and Sub
will use it for Korean, which the PP-OCRv6 recognizer cannot read. This amends
ADR-0001's OCR clause; the rest of ADR-0001 stands.

## Consequences

- Subtitle readings are stable, so a cue is translated once instead of once per
  misreading.
- Recognition costs about 50-100 ms of CPU per frame on Snapdragon X, against
  5-33 ms for Windows OCR. That fits the 250 ms capture interval but competes
  with CPU translation for cores.
- The Core executable grows by about 31 MB, and so do the repository and each
  installer. Replacing a model needs a repository change and a new Core release.
- PP-OCRv6 needs no Windows language pack for the languages it reads. Windows
  OCR language setup still applies to Korean and to users who select Windows OCR.
- The models are Apache-2.0. Distributions of Core include
  `licenses/paddleocr`.

## Alternatives considered

- **ONNX Runtime through the `ort` crate.** Its Windows prebuilt libraries
  include DirectML only, and the alternative is shipping and loading
  `onnxruntime.dll` beside Core. RTen was 32 ms against 20-24 ms for detection
  and equal for recognition, with nothing extra to ship.
- **`tract`.** Its ARM64 Windows build needs clang for assembly kernels.
- **Downloading models at build or run time.** A build-time download makes every
  Core build depend on a third-party host. A run-time download needs an install
  and repair path like HY-MT's. Vendoring costs repository size instead.
- **Model files beside the executable in the Core archive.** That changes the
  archive contents that Sub 1, Sub 2, and the release scripts verify.
- **Vision-language models.** Not evaluated; slower by design, and PP-OCRv6
  already removed most instability.

## Verification and follow-up

- Core unit tests read Chinese and English subtitle fixtures with the embedded
  models on every CI platform.
- The Rust pipeline was run over the experiment corpora and compared with the
  RapidOCR reference it reproduces. The comparison is in the experiment plan.
- Sub's switch between engines needs real Windows evidence for each engine
  before release, as every visible behavior change does.
