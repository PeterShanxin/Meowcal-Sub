# Subtitle OCR stability experiment

Status: stage 1, the Core implementation check, and an application recognition
replay measured, 2026-11-06. Stage 2 with live capture and translation has not
run. ADR-0007 records the engine decision that followed.

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


## Stage 1 first pass

### Environment

Windows 11 ARM64 (Snapdragon, 12 cores), Windows OCR languages `en-US`,
`en-GB` and `zh-Hans-CN`. Windows OCR ran natively through
`winrt-windows-media-ocr` 10.0.28000.2957 on CPython 3.13 ARM64, calling the same
`Windows.Media.Ocr` engine Core uses. RapidOCR 3.10.0 with ONNX Runtime 1.31.1
ran PP-OCR models on CPython 3.12 x64 under emulation, because OpenCV publishes
no Windows ARM64 wheel; its timings are not deployment latency. ffmpeg n8.1.2
produced all frames.

Engines, each fed identical PNG frames in timestamp order with no stabilization:

- `winocr`: raw single-pass Windows OCR, the Core `ocrRecognizeBgra` baseline.
- `winocr-mask`: the application's subtitle path - white-glyph mask plus a
  24-pixel margin (`glyph_mask.rs`), falling back to the raw frame when the
  mask yields no text. The application falls back to its configured path
  instead (`recognition_mode.rs`), which may be multi-pass or preprocessing;
  those paths and later text cleanup are not reproduced.
- `rapid-v6-small`: `PP-OCRv6_det_small` + `PP-OCRv6_rec_small`.
- `rapid-v5-mobile`: `ch_PP-OCRv5_det_mobile` + `ch_PP-OCRv5_rec_mobile`, run on
  `real-zh` only.

RapidOCR ran with orientation classification off and the detector limited to
the frame's longer side (`limit_type=max`, 1440) instead of the default
upscaling of the shorter side, which turns a subtitle strip into an
8000-pixel-wide detector input. OpenOCR and Paddle's own runtime were not
tried: RapidOCR loads the same PP-OCR weights through ONNX Runtime, which is
the runtime a Rust integration would use.

### Corpora

Media, frames, labels and per-frame outputs stay local.

- `real-zh`: 90 s (900 frames at 10 fps) of a 1080p video with burned-in
  Simplified Chinese subtitles, scaled to a 1440x810 player and cropped to the
  subtitle strip (1110x84), excluding a static channel logo. Cues were
  segmented from white-glyph pixels next to a dark outline
  (`ocr_experiment_segment_cues.py`) and labelled by hand from one frame per segment: 47 segments, 40 scored, 5
  ignored because the line was cut by the crop, 2 without a subtitle. A
  segment split or merge between those frames would not have been seen.
- `zh-nat`, `en-nat`: 20 original sentences each, rendered with libass (white,
  dark outline, shadow; one cue in three fades, one in four replaces its
  predecessor without a gap) over a looped natural scene with bright,
  low-saturation crystals, re-encoded with x264 CRF 30, scaled to 1440x810 and
  cropped to the bottom strip (`ocr_experiment_synthetic_corpus.py`). Truth and
  cue timing are exact.

A synthetic background must carry no text of its own inside the strip;
otherwise scores mostly measure which engine reads that text.

### Results

Text is compared after NFKC and the application's cue-identity normalization
(`band_cue.rs`): lowercase, letters and digits only, so whitespace and
punctuation differences do not count. Only lines at least 60% as tall as the
tallest line in the frame are kept, which drops small scene text equally for
every engine. Transition frames (cue edges and fades) are excluded; blank
frames are reported separately and do not enter the change rate. "Readings per cue" counts distinct non-empty
outputs over a cue's stable frames - each distinct reading is a potential
retranslation.

| Corpus | Engine | CER | Stable frames exact | Adjacent change rate | Readings per cue | Cue majority correct | Blank frames with output |
|---|---|---:|---:|---:|---:|---:|---:|
| `real-zh` | `winocr` | 0.044 | 70% | 17% | 2.67 | 31/40 | 0% |
| `real-zh` | `winocr-mask` | 0.064 | 67% | 7% | 1.73 | 28/40 | 0% |
| `real-zh` | `rapid-v5-mobile` | 0.000 | 100% | 0% | 1.02 | 40/40 | 0% |
| `real-zh` | `rapid-v6-small` | 0.000 | 100% | 0% | 1.00 | 40/40 | 0% |
| `zh-nat` | `winocr` | 0.147 | 39% | 54% | 8.80 | 14/20 | 2% |
| `zh-nat` | `winocr-mask` | 0.136 | 29% | 45% | 9.00 | 8/20 | 2% |
| `zh-nat` | `rapid-v6-small` | 0.077 | 87% | 9% | 2.10 | 19/20 | 50% |
| `en-nat` | `winocr` | 0.046 | 51% | 48% | 8.60 | 15/20 | 0% |
| `en-nat` | `winocr-mask` | 0.072 | 34% | 51% | 9.65 | 10/20 | 0% |
| `en-nat` | `rapid-v6-small` | 0.060 | 91% | 10% | 2.15 | 20/20 | 48% |

"Cue majority correct" is the most frequent reading over a cue's stable frames.
Stable cues last about 22 frames on the synthetic corpora.

PP-OCR's blank-frame output on the synthetic corpora is detector noise over
the crystals, short strings such as `A` or `KOOWY`, not misread subtitles.
The same noise causes most of PP-OCRv6's remaining synthetic errors, where a
taller noise box wins the 60% height rule and the subtitle line is dropped.
The application's band filter, not the recognizer, would have to reject it.
The one recurring recognition error was `脚` read as the traditional form
`腳` in 17 stable frames.

Windows OCR's errors are not random noise around a fixed reading: on the
natural-scene corpora the same unchanged subtitle reads differently in 45-54%
of adjacent frame pairs, splitting characters into radicals or substituting
look-alikes (`你今天` read as `你萨天`, `亻`, or with `今` dropped). The white
glyph mask reduces this on the real video but barely over bright scenery.
PP-OCRv6 small read the real video's 40 cues correctly on every stable frame;
PP-OCRv5 mobile missed one frame.

Model inference alone, measured natively on ARM64 with
`ocr_experiment_bench_onnx.py` (pre- and post-processing excluded):

| Model pair (ONNX Runtime 1.31.1 CPU, 4 threads) | Detector p50 | Recognizer p50 |
|---|---:|---:|
| PP-OCRv6 small | 20-24 ms | 25-29 ms |
| PP-OCRv5 mobile | 15-16 ms | 25-27 ms |
| PP-OCRv6 medium | 121 ms | 132 ms |

Inputs were a 1120x96 detector tensor and one 640x48 recognizer line. Ranges
are two runs in rotated order after the CPU was otherwise idle; a cold first run
was up to twice as slow. The x64-emulated build measured within the same range,
so emulation is not what made the Python pipeline slow (300-1000 ms per frame
including OpenCV pre- and post-processing while several runs shared the CPU).
Windows OCR on the same strips took 5-17 ms raw and 16-33 ms with the mask
(p50, native). The medium pair was not run end to end; at five times the small
pair's cost it is not a candidate for per-frame use.

## Core implementation check

[ADR-0007](../adr/0007-embedded-pp-ocr-recognizer.md) puts PP-OCRv6 small in
Core, in Rust on RTen. Before relying on it, the Rust pipeline was run over the
same corpora with the same scoring (application normalization, size filter) on
the same Snapdragon X machine, native ARM64, four inference threads.

The first version reproduced RapidOCR's defaults, including its black padding
of strips wider than 8:1. It matched the RapidOCR results within noise: real
Chinese 99.9% exact stable frames, synthetic Chinese CER 0.071, synthetic
English CER 0.059. Without the padding:

| Corpus | Pipeline | CER | Stable frames exact | Readings/cue | Cue majority correct | p50 ms | p95 ms |
| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: |
| real-zh | RapidOCR | 0 | 100% | 1.00 | 40/40 | - | - |
| real-zh | Core, no padding | 0 | 100% | 1.00 | 40/40 | 52 | 71 |
| zh-nat | RapidOCR | 0.077 | 87% | 2.10 | 19/20 | - | - |
| zh-nat | Core, no padding | 0.003 | 97% | 1.05 | 19/20 | 82 | 109 |
| en-nat | RapidOCR | 0.060 | 91% | 2.15 | 20/20 | - | - |
| en-nat | Core, no padding | 0.001 | 99% | 1.05 | 20/20 | 104 | 122 |

With padding, the detector read whole blank strips as one invented line
(`KZOOWY`), and the padded frame doubled detection time. Without it, the
remaining output on blank synthetic frames (67-81% of them) is the `Veo`
watermark in the background video: real text, not a misreading. The RapidOCR
latencies were measured under x64 emulation and are not comparable.

Capping the detector's long side at 960 instead of 1440 cut real-zh to 38 ms
but lowered synthetic Chinese stable frames to 91%, so Core keeps 1440.
Pre-packing RTen weights made no measurable difference.

## Application recognition replay

With the engine choice in the app, each corpus was read through the app's own
recognition path (`RecognitionMode` and `CoreOcr` against a source-built Core
0.2.0, ARM64 release) for each engine. A Python port of the app's text policy
(band grouping, `band_cue::Cue` admission, `RecentLines` with
`ocr_stability::classify`, a free translation slot) then replayed those reads
at the 250 ms capture interval. No translation engine ran.

The first PP-OCRv6 replay translated the synthetic English cues correctly only
2 times in 20: the background's `Veo` watermark overlapped the subtitle row,
joined its band, and was translated with it. In subtitle mode the app now drops
PP-OCRv6 lines under 60% of the tallest line's height, the size filter the
engine comparison was scored with. With it:

| Corpus | Engine | Translations per cue | Cues never translated | Last translation exact | Median onset to translation | p50 OCR ms |
| --- | --- | ---: | ---: | ---: | ---: | ---: |
| real-zh | Windows OCR, white-glyph mask | 0.93 | 3/40 | 27/40 | 300 ms | 6 |
| real-zh | PP-OCRv6 | 1.00 | 0/40 | 40/40 | 300 ms | 56 |
| zh-nat | Windows OCR, white-glyph mask | 1.05 | 2/20 | 10/20 | 900 ms | 10 |
| zh-nat | PP-OCRv6 | 1.05 | 0/20 | 19/20 | 300 ms | 88 |
| en-nat | Windows OCR, white-glyph mask | 1.15 | 0/20 | 10/20 | 350 ms | 9 |
| en-nat | PP-OCRv6 | 1.00 | 0/20 | 20/20 | 300 ms | 98 |

PP-OCRv6 started 4-5 translations outside any cue on the synthetic corpora,
against 0-1 for Windows OCR; the likely source is the watermark read alone on
frames without a subtitle, which was not checked line by line.

The same replay over the [band-gate fixture](../../evals/band-gate/README.md),
rendered headlessly at 10 frames per second with a paused page clock, gave both
engines identical outcomes: every cue of `equal-width-120s` (30) and
`negative-band-recovery-120s` (5) translated once and exactly, and two
translations outside subtitle cues in the negative-band run for each engine.
This is a replay of rendered frames, not the native capture gate.

## Remaining work

- Stage 2 with the real translation engine and live capture, including whether
  a pixel-identity cue tracker (the segmenter above recognises cue changes
  without OCR) should decide when to translate, instead of OCR text identity.
- A real English corpus and a hand-checked second Chinese source.
- Windows AI `TextRecognizer` on Copilot+ hardware was not evaluated.

## Running the experiment

All scripts are in `scripts/` and work on a corpus directory holding `frames/`.

- Real clip: `ocr_experiment_extract_frames.py VIDEO CORPUS --start S
  --duration S --crop W:H:X:Y`, then `ocr_experiment_segment_cues.py CORPUS`,
  then write `labels.csv` (`cue_id,truth`; `IGNORE` for unreadable cues, empty
  for no subtitle) from the generated `cues_sheet.png`.
- Synthetic clip: `ocr_experiment_synthetic_corpus.py BACKGROUND CORPUS --lang
  zh|en [--loop]` writes frames and `truth.csv`.
- `ocr_experiment_run.py CORPUS ENGINE --lang zh|en` writes `ocr/ENGINE.jsonl`;
  engines are `winocr`, `winocr-mask` and `rapid-<v4|v5|v6>-<size>`.
- `ocr_experiment_evaluate.py CORPUS --size-filter` scores every engine output
  in `CORPUS/ocr/` through `ocr_experiment_score.py`.
- `ocr_experiment_bench_onnx.py MODEL_DIR [THREADS]` times the ONNX models
  RapidOCR downloaded.
