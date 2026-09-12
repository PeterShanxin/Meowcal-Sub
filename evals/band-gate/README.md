# Native subtitle band gate fixture

Open [`index.html`](index.html) directly in a browser. It is self-contained: there are no network requests, bundled assets, video files, or runtime dependencies.

The page shows a moving synthetic scene with a fixed subtitle band near the bottom. Use the **120-second equal-width check** for a focused 30-event pass (30 authored English cues at four seconds each), the **120-second negative-band recovery** for same-band exclusions, and the **300-second sustained check** for a repeatable 5-minute pass. `1x live` preserves the authored timings; `2x review` and `10x smoke` are for checking the page mechanics. Keep **Repeat** enabled when sampling a short section. For the negative-band recovery scenario, use the first cycle to observe the labeled warmup and a later cycle to check that negative text does not periodically reappear as a subtitle. Disable it to let a run stop at its authored duration.

The subtitle card keeps a 720px logical width for one-line and two-line cues. The equal-width scenario keeps rendered English text at similar lengths while avoiding padded spaces or a claim that OCR sees a fixed text width. The focused equal-width and negative-band runs use English source text so native OCR comparison is unambiguous. The sustained run includes Chinese cues; select the matching Windows OCR language/tag before using those segments as evidence. The timelines also cover slow cues, short negations, numeric text, blank intervals, and reappearance after a blank. The scene also contains authored negative segments: a static watermark, a running timer, static scene text, and credits. The negative-band recovery scenario places watermark, timer, and credits inside the same subtitle card during its labeled initial warmup, then presents blank time and stable dialogue recovery; later repeats must not leak those negative readings as periodic subtitles.

`scenarios.json` is the reviewable source of truth for the authored timeline. Every segment has an ID, onset, duration, and expected role. Subtitle cues use `expected: "subtitle"`; blank intervals use `expected: "blank"`; watermark, timer, scene text, and credits use `expected: "negative"` and identify their `kind`.

For a native pass:

1. Open `index.html` and select a scenario.
2. Select the application capture region over the marked subtitle band. Keep the full subtitle card in the region and avoid relying on the page event log as OCR input.
3. Start the fixture at `1x live`, then run the application’s native OCR/band gate. Record the tested architecture, Windows build, scenario, capture dimensions, and the application band log.
4. Compare application decisions with `scenarios.json`: subtitle IDs should remain readable through slow cues, line-count transitions, Chinese cues, blanks, and reappearance; negative IDs should not be admitted as subtitles.

The page’s `data-role`, `data-expected`, `data-cue-id`, `data-onset`, and `data-duration` attributes make the current authored state inspectable without changing the visible scene. The event log is an operator aid only. This fixture demonstrates the visual input and authored expectation; it does not by itself prove Windows capture, Windows OCR, native overlay, or application-level translation.


## Offline gate report

After a native run, save the content-aware application band log as JSONL with a `utc_ms` field on every gate frame, and save `window.fixture.readState()` as `fixture-state.json`. The state must come from a completed 1x run with Repeat disabled. The report uses `timeOriginMs + runStartedAtMs` to align authored onset windows with the UTC gate frames:

```powershell
node .\evals\band-gate\report.mjs .\gate-log.jsonl .\fixture-state.json .\translation-events.json
```

The report reads only `kind: "gate"` entries from a mixed JSONL log, compares normalized OCR source text per authored cue, records missed and correctly admitted cue IDs, and never counts an admitted frame whose OCR text is wrong. It computes first-correct-OCR to first-admission p50/p95 delays and counts negative admissions after each segment's warmup allowance. A negative segment also needs post-warmup frame coverage spanning at least half of its remaining authored interval; a zero-frame or one-frame negative sample cannot prove sustained exclusion. Optional translation events count as `trueTranslated` only when the original source matches an authored cue and `displayState` is exactly `translated` with a non-empty target. When translation events are supplied, every authored subtitle cue must have true translated evidence for `endToEndOk`; the OCR gate result remains separately visible as `gateOk`. A non-empty target in any other state is recorded as rejected evidence. Paused, incomplete, missing-timebase, or non-1x runs are reported as partial and cannot pass the gate.

The small deterministic report checks run with:

```powershell
node --test .\evals\band-gate\report.test.mjs
```
