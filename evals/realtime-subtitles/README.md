# Real-time subtitle policy replay

Run on Windows with Git and the repository Rust toolchain:

```powershell
.\evals\realtime-subtitles\run.ps1 -OutputPath .local/realtime-report.json
```

The runner reads the pre-FIFO policies from baseline commit `31882a0` and the
current queue/pacing sources. `-BaselineRef` selects another compatible baseline.
Temporary source snapshots and the executable are removed after the run. The
report records both commits and whether the candidate tree contains tracked edits.

The authored inputs cover 500/600/800/1000 ms dialogue, negations, changed numbers,
single Chinese characters, returning dialogue, one-frame OCR noise, a paused
source, busy inference, late completion, long translations of short dialogue
and severe-stall recovery. Inference times are deterministic (350–650 ms normally, an initial 1800 ms busy request,
and 4000 ms requests during the overload interval). Capture/OCR work is simulated
as 50 ms. Short replies use a three-character target; the OCR-noise and number
cases exercise longer targets. A two-minute case uses a long English target
for one-second Chinese dialogue to detect display-induced backlog. This measures
policy behavior, not model quality or device performance.

Normal cases require every cue to display exactly once, in order, with no skips,
an empty final queue, depth at most four and at least 350 ms of visible time for brief replies.
The severe-stall case requires counted skipping, recovery to the final cue and
an empty final queue. The report includes missed/duplicate/reordered cues,
latency p50/p95, minimum visibility, depth, skip count and OCR sample count.

The replay does not exercise spatial filtering, Windows capture, native OCR,
inference, compositor visibility or resource consumption. Run the
[native band gate](../band-gate/README.md) and real-media validation separately.
