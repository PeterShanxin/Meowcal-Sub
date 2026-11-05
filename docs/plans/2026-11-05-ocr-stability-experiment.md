# Subtitle OCR stability experiment

Status: proposed, 2026-11-05. No real video has been captured, candidate OCR
engines installed, or OCR measurements run. This plan does not change the
production engine.

## Question and scope

Measure whether complete OCR pipelines produce more accurate and consistent
subtitle text across successive frames, and whether any improvement survives
the application's subtitle and translation policies. Meowcal Core owns native
OCR; the application owns capture, filtering, translation policy and display.
Use an isolated experimental branch and test profile. Do not replace the
installed engine or change production behavior.

## Corpus

Record one complete episode locally, without audio, using the user-authorized
rectangle spanning the video width and both subtitle lines. Use ordinary
download if available; otherwise capture only the selected rectangle. Do not
bypass content protection. Keep the window fixed and visible, preserve aspect
ratio, and record the rectangle, capture method, resolution, frame rate, source
timing, dropped frames and hash. Stop if another window enters the rectangle,
the picture is black, or the rectangle moves. Keep the source locally and do not
commit media, subtitle text or logs.

Choose representative intervals from the recording before comparing engines.
Include stable lines, changes, blank intervals, fades and difficult backgrounds.
Decode the same consecutive frames to lossless images for every engine, retain
presentation timestamps and record extraction commands and versions. Preserve
the full two-line capture. For a single-language run, crop the other line out
before OCR using the same documented, reproducible sub-rectangle for all
engines. Manually label each cue's text and timing, including unreadable and
transition intervals; do not score uncertain labels as ground truth.

## Stage 1: frame-level comparison

Compare Windows native OCR against viable complete PP-OCRv6 Small,
PP-OCRv6 Medium and OpenOCR pipelines. Each specialized candidate needs text
detection, region extraction, recognition and decoding; SVTRv2 alone is a
recognizer, not a complete OCR pipeline. Record model and weight hashes,
detector, recognizer, runtime, configuration, language and architecture. Verify
the complete combination runs on Windows ARM64 and x64; report native versus
emulated execution separately. Verify language support for the actual selected
line. Unsupported or failed configurations remain unavailable, not ranked.

Feed identical pixels to each pipeline in timestamp order. Bypass temporal
stabilization, deduplication, translation and display. Use the Core
`ocrRecognizeBgra` raw Windows OCR call as the baseline. Report the existing
application preprocessing or multi-pass policy separately if evaluated; it is
not a raw single-pass baseline.

Report per-frame outputs, failures and timeouts alongside:

- character error rate against manually checked text, with blank-frame false
  positives reported separately;
- adjacent-frame output-change rate while the ground-truth cue is unchanged,
  both exact and under one declared normalization rule;
- time from a labeled cue change to the first three consecutive correct frames,
  plus the fraction of cues that never reach that condition;
- end-to-end detection-through-decoding latency P50 and P95, cold startup,
  CPU use and peak memory, with hardware and sampling method.

Separate stable and transition frames, report frame and cue counts, and retain
per-cue distributions. Use identical warmup and repeated runs, rotate engine
order, and include failed frames rather than excluding them. Record installed
Windows OCR languages, runtime versions and model licensing before testing.
Published model benchmarks are not results for this corpus or machine.

The existing Sub 2 synthetic benchmark exercises a complete application OCR
policy and is not a direct comparison with these raw real-video frame results.

## Stage 2: isolated application replay

Select candidates using stage 1 accuracy, stability, latency and resource
results. Replay the same timestamped frames through an isolated test build and
the same capture cadence, postprocessing and translation configuration for
each candidate. Keep the production installation and profile untouched. Record
trigger counts per cue, repeated and missed translations, wrongly merged cues,
displayed subtitle errors, time to first correct display and resource use.
Use a deterministic translation stub to isolate triggering, then compare real
translation under the same settings where available. Report raw OCR results
and final application outcomes separately.

## Candidate references

- [PP-OCRv6 model family](https://paddlepaddle.github.io/PaddleOCR/main/en/version3.x/algorithm/PP-OCRv6/PP-OCRv6.html):
  Small and Medium provide detection and recognition models; the recognition
  models list Chinese, English, Japanese and other languages.
- [Paddle installation requirements](https://www.paddlepaddle.org.cn/documentation/docs/en/install/index_en.html):
  the documented Windows processor requirement is x64; do not assume native
  Windows ARM64 support from a model's availability.
- [OpenOCR](https://github.com/topdu/OpenOCR): the described system supports
  Chinese and English detection and recognition. Verify the selected detector,
  recognizer, runtime and Windows architecture rather than assuming support.


## Adapter feasibility checkpoint

The host is Windows ARM64; the available Python 3.12 interpreter reports AMD64
and therefore runs under x64 emulation. Its environment did not contain Pillow,
Paddle, PyTorch, ONNX Runtime or winocr. The Windows baseline requires a
compatible, explicitly version-pinned Core executable and an installed language
pack; a historical ARM64 Core 0.1.4 package was located but not run or established
as the current baseline.

The PaddleOCR pipeline accepts explicit detection and recognition model names.
Pair `PP-OCRv6_small_det` with `PP-OCRv6_small_rec`, and
`PP-OCRv6_medium_det` with `PP-OCRv6_medium_rec`. Disable optional document
orientation, unwarping and text-line orientation for the initial comparison;
record that choice. The published pipeline can use different inference engines,
but neither an ARM64 native combination nor x64 emulation has been verified here.

OpenOCR's [quick start](https://github.com/Topdu/OpenOCR/blob/main/QUICKSTART.md)
documents `openocr-python==0.1.5` and `openocr --task ocr` for detection plus
recognition. Its server example uses the Torch backend; the project also offers
ONNX export. Confirm which detector and SVTRv2 weights the selected package
actually loads and record them before accepting a result. Its documented system
supports Chinese and English; no other language is assumed. Windows ARM64 and
x64 package, model and backend combinations remain unverified.

On this machine, separate `pip download --no-deps --timeout 5 --retries 0`
probes for `openocr-python==0.1.5`, `paddleocr==3.7.0` and `paddlepaddle`
each returned “No matching distribution found” from the configured package
source. This does not establish that upstream has no compatible packages or
identify why this source returned no versions. No candidate was installed or
executed. Obtain verified dependencies and weights in an isolated environment
before building or evaluating adapters.

## Preparation and remaining work

The frame extraction and scoring helpers in `scripts/ocr_experiment_*.py` were
checked only with synthetic inputs. Those checks are not OCR or real-video
evidence. Extract frames with `python scripts/ocr_experiment_extract_frames.py
VIDEO NEW_DIRECTORY --start SECONDS --duration SECONDS --crop W:H:X:Y`.
Omit `--crop` for both lines. The helper writes PNGs, source timestamps and a
manifest. Inspect the output before use. The scorer accepts a time-ordered CSV
with `seconds,cue_id,cue_start_seconds,truth,output,phase`; use a unique cue ID
for each appearance, `ignore` for uncertain frames, and empty truth for blank
frames. Run `python scripts/ocr_experiment_score.py LABELS.csv`. The scorer
reports frame-level character error, output changes and three-frame stability;
it does not measure latency, process resources or final application behavior.
Before implementation,
confirm the source recording and its timing, verify dependencies and complete
pipeline availability, define evaluation thresholds, and build a reproducible
frame runner and scorer. Review a draft PR without merging or deploying it
until the actual experiment and appropriate Windows validation are complete.
