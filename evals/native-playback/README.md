# Native playback regression

Generate an authored MP4 with burned-in subtitles and its cue manifest.
The same source cues also run in the [native band fixture](../band-gate/README.md).
This adds real video decoding and player behavior, not a recording of the
browser fixture. It requires FFmpeg with libass and libx264.

```powershell
.\evals\native-playback\make-fixture.ps1 -OutputDirectory .local\rapid-playback
```

For a steady-dialogue comparison, pass `-Scenario equal-width-120s`. Use
`single-character-12s` with Chinese recognition to check short Chinese cues.
Open `fixture.mp4` in the usual Windows player. Select its subtitle band in the
native app and run at 1x without seek or pause for the baseline. Compare the
overlay and video against `scenario.json`; the SRT is ground truth, not input to
the translation app. Record late, omitted, duplicate and out-of-order
translations, and separate visible translation from source-only notices.

## Coverage matrix

Test each shipped setting and the combinations most likely to change the
result. When a feature changes, add its affected combinations here, rather
than requiring every setting to be crossed with every other one.

| Risk | Cases | Required result |
| --- | --- | --- |
| Diagnostics | Off, Basic, Verbose with an empty directory, then Verbose with populated history; flip levels during playback | Capture and subtitle display keep moving. Off and Basic create no new image records; a stalled writer skips evidence rather than blocking a cue. |
| Recognition | PP-OCRv6 and Windows OCR; English, Chinese, and Korean dispatch; blank, repeated and short cues | The expected recognizer is used. No ordinary status spam; do not mask a real recognizer error. No subtitles invented from blank frames. |
| Viewer controls | Pause/resume, seek, Stop/Start, move or resize the capture area | New cues recover; delayed results from the previous position or session do not remain on screen. |
| Sustained use | Short dialogue pass, five-minute moving fixture, and a longer real-media session with accumulated records | Compare recall, p50/p95 time to display, queue/skip totals and app/Core/model memory at start, steady state and after Stop. |
| Resource and runtime faults | Busy writer, unavailable model, inference timeout and recovery; restrained tests under memory load on a disposable bench | The app remains responsive, late translations do not block new cues, and a functioning engine can resume without stale output. |
| Platform | Direct and Store builds on x64 and ARM64 as the change affects them | Label the exact build and hardware used. An x64 or debug result does not certify the ARM64 Store build. |

For the logging regression, run the same single-language clip in Off, Basic,
and Verbose on the same build. Use comparable disposable profiles. Re-run
Verbose with prefilled history and repeat a Stop/Start. Keep the capture band,
OCR engine, model, acceleration, and clip speed fixed; warm the model before
measuring. Start a fresh capture session for each run to clear duplicate and
recent-translation state. Restore the same starting history snapshot for each
mode within a clean-history or populated-history comparison. Alternate the
order across repetitions to reduce warm-up bias.
Use the [storage microbenchmark](../diagnostic-logging/README.md) to measure
writer pressure independently of the model.

Logging mode must not cause extra missed or wrong-order translations or a
repeatable increase in visible subtitle delay. Compare cue IDs and source-to-
overlay latency against Off and Basic, not only the total displayed count.
If the Off and Basic repeats disagree, record their variation and investigate
before interpreting a one-off Verbose difference. Perform the same comparison
on the old build if reproducing the regression.

### Log level change with a busy writer

Use a disposable profile. Start in Verbose, then hold its existing
`verbose/.writer.lock` briefly while the clip is playing. Switch to Off before
releasing the lock. Check that the overlay continues without waiting. Use
different cue ranges for the two modes; compare the evidence record capture
IDs and timestamps, not just directory modification times. Frames captured
while Off must not produce new verbose evidence. Switch back to Verbose and
confirm that fresh records resume.

### Measure what the viewer sees

Record the player and translated overlay together on one desktop timeline.
For each source cue, compare its first visible frame to the first frame that
shows its matching translation. Count late, absent, and wrong-order cues;
report timing precision as one frame at the actual recording rate. Lead with
on-time, late, and absent counts against the full cue manifest, and flag
duplicates and wrong-order cues separately. Compare latency by cue ID across
modes; a percentile taken only from displayed cues can look better if short
cues were missed. Use
capture, OCR, and translation events to locate failures, not as proof of
display. A translation event without visible matching text is a display miss.
