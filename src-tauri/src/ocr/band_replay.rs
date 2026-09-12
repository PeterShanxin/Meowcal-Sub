// =============================================================================
// BAND_REPLAY.RS - running a recorded session back through the real tracker
// =============================================================================
// Operator-run, asserts nothing, `#[ignore]`d. Same shape as `preprocessing_ab`.
//
//   $env:MEOWCAL_BAND_REPLAY = "D:\bands.jsonl"
//   $env:MEOWCAL_BAND_REGION = "1832"        # optional, capture width in px
//   cargo test --lib ocr::band_replay -- --ignored --nocapture
//
// This exists because the offline analysis that produced the thresholds and the
// tracker that has to apply them are not the same thing, and the difference
// matters. The analysis summarised a whole session at once; the tracker keeps a
// ninety-second window and re-judges continuously. A band that is a subtitle for
// twenty minutes and then carries end credits reads, to a whole-session average,
// as neither - which is exactly what the second recorded session produced. Only
// replaying through the real window shows what the app would actually have done.
//
// Content-aware replay requires gate frames recorded with both
// MEOWCAL_BAND_LOG and MEOWCAL_BAND_LOG_TEXT=1. Count-only historical logs
// cannot prove content identity and are rejected rather than fabricated.
use super::band_tracker::BandTracker;
use super::band_verdict::Verdict;
use super::LineBox;

struct Frame {
    at_ms: u64,
    texts: Vec<String>,
    boxes: Vec<LineBox>,
}

fn load(path: &str) -> Result<Vec<Frame>, Box<dyn std::error::Error>> {
    parse(&std::fs::read_to_string(path)?)
}

fn parse(raw: &str) -> Result<Vec<Frame>, Box<dyn std::error::Error>> {
    let mut frames = Vec::new();
    for (index, line) in raw.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let invalid = |message: &str| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("band log line {}: {message}", index + 1),
            )
        };
        let entry: serde_json::Value = serde_json::from_str(line)?;
        if entry["kind"] != "gate" {
            continue;
        }
        let at_ms = entry["ms"].as_u64().ok_or_else(|| invalid("missing ms"))?;
        let lines = entry["lines"]
            .as_array()
            .ok_or_else(|| invalid("missing lines"))?;
        let mut frame = Frame {
            at_ms,
            texts: Vec::new(),
            boxes: Vec::new(),
        };
        for line in lines {
            frame.texts.push(
                line["text"]
                    .as_str()
                    .ok_or_else(|| {
                        invalid(
                "content replay requires MEOWCAL_BAND_LOG_TEXT=1; count-only logs are insufficient"
            )
                    })?
                    .to_owned(),
            );
            let coordinate = |key: &str| {
                line[key]
                    .as_f64()
                    .filter(|value| value.is_finite())
                    .ok_or_else(|| invalid("missing or invalid line geometry"))
            };
            frame.boxes.push(LineBox {
                x: coordinate("x")? as f32,
                y: coordinate("y")? as f32,
                width: coordinate("w")? as f32,
                height: coordinate("h")? as f32,
            });
        }
        frames.push(frame);
    }
    if frames.is_empty() {
        return Err("log contains no content-aware gate frames".into());
    }
    Ok(frames)
}

/// Which tenth of the session a frame falls in, so a band that changes
/// character partway through is visible rather than averaged away.
fn decile(index: usize, total: usize) -> usize {
    (index * 10 / total.max(1)).min(9)
}

#[test]
#[ignore = "operator-run against a recorded session"]
fn replay_a_recorded_session() -> Result<(), Box<dyn std::error::Error>> {
    let Ok(path) = std::env::var("MEOWCAL_BAND_REPLAY") else {
        println!("set MEOWCAL_BAND_REPLAY to a band log path");
        return Ok(());
    };
    let region_width: f32 = std::env::var("MEOWCAL_BAND_REGION")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(1832.0);

    let frames = load(&path)?;
    // The median gap, not the smallest. The log only records frames that
    // reached the gate, so gaps vary; but the smallest gap is a burst
    // artefact, and using it would understate screen time and inflate every
    // cue rate in proportion. The live pipeline knows its capture interval
    // outright - this is only reconstructing it.
    let mut gaps: Vec<u64> = frames
        .windows(2)
        .map(|pair| pair[1].at_ms.saturating_sub(pair[0].at_ms))
        .collect();
    gaps.sort_unstable();
    let interval = gaps.get(gaps.len() / 2).copied().unwrap_or(250).max(1);

    let mut tracker = BandTracker::new(region_width, interval);
    // Keyed by the band's rounded height, since a band drifts a few pixels.
    let mut kept: std::collections::BTreeMap<i64, [usize; 10]> = Default::default();
    let mut held: std::collections::BTreeMap<i64, [usize; 10]> = Default::default();
    let mut lines_admitted = 0usize;
    let mut lines_held = 0usize;
    // A cue is a stretch of consecutive frames in which a band held text. One
    // where not a single frame was admitted is a subtitle that never
    // appeared, which is what "a quick line got nothing" looks like from here.
    // Frame percentages cannot show it: a cue can lose most of its frames and
    // still be displayed from the rest.
    #[derive(Default)]
    struct Runs {
        total: usize,
        lost: usize,
        frames: usize,
        lost_frames: usize,
        run_admitted: bool,
        run_frames: usize,
        last_index: usize,
    }
    impl Runs {
        fn close(&mut self) {
            if self.run_frames > 0 && !self.run_admitted {
                self.lost += 1;
                self.lost_frames += self.run_frames;
            }
        }
    }
    let mut runs: std::collections::BTreeMap<i64, Runs> = Default::default();

    for (index, frame) in frames.iter().enumerate() {
        let slot = decile(index, frames.len());
        let banding = tracker.observe(&frame.texts, &frame.boxes, frame.at_ms);
        let mut note = |y: i64, admitted: bool| {
            let entry = runs.entry(y).or_default();
            if entry.last_index != index {
                // A gap since this band was last live ends the stretch.
                entry.close();
                entry.total += 1;
                entry.run_admitted = false;
                entry.run_frames = 0;
            }
            entry.run_admitted |= admitted;
            entry.run_frames += 1;
            entry.frames += 1;
            entry.last_index = index + 1;
        };

        for group in &banding.included {
            if group.centre_y.is_nan() {
                continue;
            }
            let y = (group.centre_y / 20.0).round() as i64 * 20;
            kept.entry(y).or_default()[slot] += 1;
            lines_admitted += group.lines.len();
            note(y, true);
        }
        for band in &banding.dropped {
            let y = (band.centre_y / 20.0).round() as i64 * 20;
            held.entry(y).or_default()[slot] += 1;
            lines_held += band.lines;
            note(y, false);
        }
    }
    // Close the final stretch of every band.
    for entry in runs.values_mut() {
        entry.close();
    }

    println!("\n{path}");
    println!(
        "{} frames, {:.1} min, {interval} ms apart, region {region_width:.0} px wide",
        frames.len(),
        frames.last().unwrap().at_ms as f64 / 60_000.0
    );
    println!(
        "\n{lines_admitted} lines admitted, {lines_held} held back ({:.1}% held)",
        lines_held as f64 / (lines_admitted + lines_held).max(1) as f64 * 100.0
    );

    println!("\n     y   admitted-by-decile   held-by-decile");
    let rows: std::collections::BTreeSet<i64> = kept.keys().chain(held.keys()).copied().collect();
    for y in rows {
        let show = |counts: Option<&[usize; 10]>| match counts {
            None => "..........".to_string(),
            Some(counts) => counts
                .iter()
                .map(|n| {
                    if *n == 0 {
                        '.'
                    } else {
                        char::from_digit(((*n as f64).log10() * 3.0).min(9.0) as u32, 10).unwrap()
                    }
                })
                .collect(),
        };
        let total =
            |counts: Option<&[usize; 10]>| counts.map_or(0, |counts| counts.iter().sum::<usize>());
        let (admitted, withheld) = (total(kept.get(&y)), total(held.get(&y)));
        // A band that is sometimes admitted and sometimes not, in the same
        // stretch, is a verdict flickering on a threshold - which shows up as a
        // subtitle that intermittently vanishes. Worth seeing per band.
        let flicker = if admitted > 0 && withheld > 0 {
            format!(
                "  {:.0}% held",
                withheld as f64 / (admitted + withheld) as f64 * 100.0
            )
        } else {
            String::new()
        };
        println!(
            "{y:6}   {}            {}   {admitted:6} {withheld:6}{flicker}",
            show(kept.get(&y)),
            show(held.get(&y))
        );
    }

    println!("\nstretches never admitted at all - text on screen that showed nothing.");
    println!("Weighted by frames, because losing a two-frame blip is not losing a subtitle.");
    println!("\n     y   stretches lost   screen time lost");
    for (y, entry) in &runs {
        if entry.total < 5 {
            continue;
        }
        println!(
            "{y:6}   {:5} of {:5}  {:3.0}%   {:6} of {:6}  {:3.0}%",
            entry.lost,
            entry.total,
            entry.lost as f64 / entry.total as f64 * 100.0,
            entry.lost_frames,
            entry.frames,
            entry.lost_frames as f64 / entry.frames.max(1) as f64 * 100.0
        );
    }

    println!("\nfinal verdicts:");
    for (centre_y, verdict) in tracker.verdicts() {
        println!("  y {centre_y:7.0}  {verdict:?}");
    }
    let _ = Verdict::Subtitle;
    Ok(())
}

#[test]
fn replay_rejects_logs_without_content_or_valid_geometry() {
    for raw in [
        "{",
        r#"{"kind":"line_geometry","lines":[]}"#,
        r#"{"kind":"gate","ms":0,"lines":[{"chars":10}]}"#,
        r#"{"kind":"gate","ms":0,"lines":[{"text":"Hello"}]}"#,
    ] {
        assert!(parse(raw).is_err(), "{raw}");
    }
    assert!(parse(r#"{"kind":"gate","ms":0,"lines":[]}"#).is_ok());
}
