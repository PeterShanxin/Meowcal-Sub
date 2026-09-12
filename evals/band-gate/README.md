# Native subtitle band gate fixture

Open [`index.html`](index.html) directly in a browser. It is self-contained: there are no network requests, bundled assets, video files, or runtime dependencies.

The page shows a moving synthetic scene with a fixed subtitle band near the bottom. Use the **120-second equal-width check** for a focused 30-event pass (30 authored English cues at four seconds each), the **120-second negative-band recovery** for same-band exclusions, and the **300-second sustained check** for a repeatable 5-minute pass. `1x live` preserves the authored timings; `2x review` and `10x smoke` are for checking the page mechanics. Keep **Repeat** enabled when sampling a short section. For the negative-band recovery scenario, use the first cycle to observe the labeled warmup and a later cycle to check that negative text does not periodically reappear as a subtitle. Disable it to let a run stop at its authored duration.

The subtitle card keeps a 720px logical width for one-line and two-line cues. The equal-width scenario keeps rendered English text at similar lengths while avoiding padded spaces or a claim that OCR sees a fixed text width. The focused equal-width and negative-band runs use English source text so native OCR comparison is unambiguous. The sustained run includes Chinese cues; select the matching Windows OCR language/tag before using those segments as evidence. The timelines also cover slow cues, short negations, numeric text, blank intervals, and reappearance after a blank. The scene also contains authored negative segments: a static watermark, a running timer, static scene text, and credits. The negative-band recovery scenario places watermark, timer, and credits inside the same subtitle card during its labeled initial warmup, then presents blank time and stable dialogue recovery; later repeats must not leak those negative readings as periodic subtitles.

`scenarios.json` is the reviewable source of truth for the authored timeline. Every segment has an ID, onset, duration, and expected role. Subtitle cues use `expected: "subtitle"`; blank intervals use `expected: "blank"`; watermark, timer, scene text, and credits use `expected: "negative"` and identify their `kind`.

For a native pass:

1. Open `index.html` and select a scenario.
2. Select the application capture region over the marked subtitle band. Keep the full subtitle card in the region and avoid relying on the page event log as OCR input.
3. Wait for the application engine to be ready, start capture, then start the fixture at `1x live`. Record the tested architecture, Windows build, scenario, capture dimensions, and the application band log.
4. Compare application decisions with `scenarios.json`: subtitle IDs should remain readable through slow cues, line-count transitions, Chinese cues, blanks, and reappearance; negative IDs should not be admitted as subtitles.

Enable local gate evidence before launching the development application:

```powershell
New-Item -ItemType Directory -Path .local -Force | Out-Null
$env:MEOWCAL_BAND_LOG = Join-Path (Get-Location) '.local/band-gate.jsonl'
$env:MEOWCAL_BAND_LOG_TEXT = '1'
.\dev-tauri.cmd
```

Use a new log filename for each application launch. Select the matching source
language, keep **Translate any text** off, and wait for **Engine ready** before
starting each translation session. In the fixture's browser console, save
`JSON.stringify(window.fixture.readState(), null, 2)` after completion.
Check `scenarioId` against the selected scenario before comparing results.

To record translation evidence, run this once in the development application's
main-window developer console before starting capture, then save
`JSON.stringify(window.bandGateTranslations, null, 2)` after completion:

```javascript
window.bandGateTranslations = [];
await window.__TAURI__.event.listen('translation-update', event => {
  window.bandGateTranslations.push({ receivedAt: Date.now(), payload: event.payload });
});
```

These logs contain source and target text. Keep them outside version control;
publish aggregate results and screenshots of the authored fixture only.

The page's `data-role`, `data-expected`, `data-cue-id`, `data-onset`, and
`data-duration` attributes expose authored state for inspection. The event log
is an operator aid; it does not prove Windows capture, OCR, native overlay, or
application-level translation.


## Offline gate report

After a native run, save the content-aware application band log as JSONL with a `capture_utc_ms` field recorded before capture/OCR on every gate frame, and save `window.fixture.readState()` as `fixture-state.json`. The state must come from a completed 1x run with Repeat disabled. The report rejects older logs that only timestamp processing completion. It uses `timeOriginMs + runStartedAtMs` to align authored onset windows with the UTC gate frames:

```powershell
node .\evals\band-gate\report.mjs .\gate-log.jsonl .\fixture-state.json .\translation-events.json
```

The report reads only `kind: "gate"` entries from a mixed JSONL log and compares normalized OCR source text per authored cue. Raw OCR lines establish recognition; admission evidence must use the frame's `admitted_texts: string[]`, containing only the lines forwarded by the native filter. A missing `admitted_texts` field makes the report partial, and a partial forwarded line cannot admit a multi-line cue. The report records missed and correctly admitted cue IDs and computes first-correct-OCR to first-admission p50/p95 delays.

Negative admissions are counted only when non-empty forwarded text appears after each segment's warmup allowance. A negative segment also needs at least two non-empty post-warmup frames spanning at least half of its remaining authored interval; empty OCR frames do not prove sustained exclusion. Optional translation events count as `trueTranslated` only when the original source matches an authored subtitle capture window and `displayState` is exactly `translated` with a non-empty target. Capture time is derived from numeric `payload.timestamp - payload.totalMs`; events captured in negative or out-of-window intervals are ignored for subtitle mismatch evidence. When translation events are supplied, every authored subtitle cue must have true translated evidence for `endToEndOk`; the OCR gate result remains separately visible as `gateOk`. A non-empty target in another state is recorded as rejected evidence. Paused, incomplete, missing-timebase, or non-1x runs are reported as partial and cannot pass the gate.

Warmup starts at the first non-empty OCR observation of a negative segment.
This separates capture startup latency from the gate's hold duration. Fixture
onset drift beyond 250ms makes the report partial, since nominal timestamps
can no longer reliably identify the visible cue.

Any forwarded text during an authored blank fails the gate. Negative exclusion
measures new admissions; the existing `bandHeld` display policy may retain a
translation admitted during warmup, so zero later admissions does not prove
that the overlay is empty.

The small deterministic report checks run with:

```powershell
node --test .\evals\band-gate\report.test.mjs
```
