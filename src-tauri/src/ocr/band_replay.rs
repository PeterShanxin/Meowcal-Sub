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
// The log stores a character count rather than the text, so the text is rebuilt
// as that many filler characters. `BandTracker` only ever counts them, so this
// is exact for its purposes and keeps `docs/evidence/README.md` satisfied: no
// recorded text existed to leak.
// =============================================================================

use super::band_tracker::BandTracker;
use super::band_verdict::Verdict;
use super::LineBox;

struct Frame {
    at_ms: u64,
    texts: Vec<String>,
    boxes: Vec<LineBox>,
}

fn load(path: &str) -> Vec<Frame> {
    let raw = std::fs::read_to_string(path).expect("the band log should be readable");
    raw.lines()
        .filter(|line| !line.trim().is_empty())
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .map(|entry| {
            let lines = entry["lines"].as_array().cloned().unwrap_or_default();
            Frame {
                at_ms: entry["ms"].as_u64().unwrap_or(0),
                texts: lines
                    .iter()
                    .map(|line| "x".repeat(line["chars"].as_u64().unwrap_or(0) as usize))
                    .collect(),
                boxes: lines
                    .iter()
                    .map(|line| LineBox {
                        x: line["x"].as_f64().unwrap_or(0.0) as f32,
                        y: line["y"].as_f64().unwrap_or(0.0) as f32,
                        width: line["w"].as_f64().unwrap_or(0.0) as f32,
                        height: line["h"].as_f64().unwrap_or(0.0) as f32,
                    })
                    .collect(),
            }
        })
        .collect()
}

/// Which tenth of the session a frame falls in, so a band that changes
/// character partway through is visible rather than averaged away.
fn decile(index: usize, total: usize) -> usize {
    (index * 10 / total.max(1)).min(9)
}

#[test]
#[ignore = "operator-run against a recorded session"]
fn replay_a_recorded_session() {
    let Ok(path) = std::env::var("MEOWCAL_BAND_REPLAY") else {
        println!("set MEOWCAL_BAND_REPLAY to a band log path");
        return;
    };
    let region_width: f32 = std::env::var("MEOWCAL_BAND_REGION")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(1832.0);

    let frames = load(&path);
    assert!(!frames.is_empty(), "the log held no frames");
    // The median gap, not the smallest. The log only records frames that
    // recognised something, so gaps vary; but the smallest gap is a burst
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
    let mut lines_translated = 0usize;
    let mut lines_held = 0usize;

    for (index, frame) in frames.iter().enumerate() {
        let slot = decile(index, frames.len());
        let banding = tracker.observe(&frame.texts, &frame.boxes, frame.at_ms);
        for group in &banding.included {
            if group.centre_y.is_nan() {
                continue;
            }
            kept.entry((group.centre_y / 20.0).round() as i64 * 20)
                .or_default()[slot] += 1;
            lines_translated += group.lines.len();
        }
        for band in &banding.dropped {
            held.entry((band.centre_y / 20.0).round() as i64 * 20)
                .or_default()[slot] += 1;
            lines_held += band.lines;
        }
    }

    println!("\n{path}");
    println!(
        "{} frames, {:.1} min, {interval} ms apart, region {region_width:.0} px wide",
        frames.len(),
        frames.last().unwrap().at_ms as f64 / 60_000.0
    );
    println!(
        "\n{lines_translated} lines translated, {lines_held} held back ({:.1}% held)",
        lines_held as f64 / (lines_translated + lines_held).max(1) as f64 * 100.0
    );

    println!("\n     y   translated-by-decile   held-by-decile");
    let rows: std::collections::BTreeSet<i64> =
        kept.keys().chain(held.keys()).copied().collect();
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
        let total = |counts: Option<&[usize; 10]>| {
            counts.map_or(0, |counts| counts.iter().sum::<usize>())
        };
        let (translated, withheld) = (total(kept.get(&y)), total(held.get(&y)));
        // A band that is sometimes translated and sometimes not, in the same
        // stretch, is a verdict flickering on a threshold - which shows up as a
        // subtitle that intermittently vanishes. Worth seeing per band.
        let flicker = if translated > 0 && withheld > 0 {
            format!(
                "  {:.0}% held",
                withheld as f64 / (translated + withheld) as f64 * 100.0
            )
        } else {
            String::new()
        };
        println!(
            "{y:6}   {}            {}   {translated:6} {withheld:6}{flicker}",
            show(kept.get(&y)),
            show(held.get(&y))
        );
    }

    println!("\nfinal verdicts:");
    for (centre_y, verdict) in tracker.verdicts() {
        println!("  y {centre_y:7.0}  {verdict:?}");
    }
    let _ = Verdict::Subtitle;
}
