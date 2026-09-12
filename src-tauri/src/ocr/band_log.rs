//! Opt-in local band diagnostics. MEOWCAL_BAND_LOG records geometry, identity
//! digests, and gate decisions. MEOWCAL_BAND_LOG_TEXT=1 additionally stores OCR
//! text for content-aware replay; these local files must never be published.
use super::LineBox;
use std::hash::{Hash, Hasher};
use std::io::Write;
use std::sync::OnceLock;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

/// Gate frames use the filter's clock and include empty observations. Text is
/// retained only with the additional explicit local-diagnostic opt-in.
pub(super) fn record_gate(
    result: &super::OcrResult,
    at_ms: u64,
    captured_at: SystemTime,
    decisions: &[super::banding::BandDecision],
    admitted_texts: &[String],
) {
    let Some(path) = target() else {
        return;
    };
    let include_text = std::env::var("MEOWCAL_BAND_LOG_TEXT").as_deref() == Ok("1");
    let lines: Vec<_> = result
        .lines
        .iter()
        .zip(&result.boxes)
        .map(|(text, area)| {
            let mut line = serde_json::json!({
                "x": area.x, "y": area.y, "w": area.width, "h": area.height,
                "chars": text.chars().count(), "digest": format!("{:016x}", digest(text))
            });
            if include_text {
                line["text"] = serde_json::json!(text);
            }
            line
        })
        .collect();
    let decisions: Vec<_> = decisions
        .iter()
        .map(|d| {
            serde_json::json!({
                "band_id": d.band_id, "cue_id": d.cue_id,
                "raw": format!("{:?}", d.raw), "settled": format!("{:?}", d.settled),
                "cue_changed": d.cue_changed, "recovered": d.recovered,
                "reason": d.settled.reason(), "admitted": d.settled.is_included()
            })
        })
        .collect();
    let mut entry = serde_json::json!({
        "kind": "gate", "ms": at_ms,
        "utc_ms": SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis(),
        "capture_utc_ms": captured_at.duration_since(UNIX_EPOCH).unwrap_or_default().as_millis(),
        "frame_width": result.frame_width,
        "lines": lines, "decisions": decisions
    });
    if include_text {
        entry["admitted_texts"] = serde_json::json!(admitted_texts);
    }
    if let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
    {
        let _ = writeln!(file, "{entry}");
    }
}

/// Resolved once. `None` means the instrument is off, which is the normal case.
static TARGET: OnceLock<Option<String>> = OnceLock::new();
static START: OnceLock<Instant> = OnceLock::new();

fn target() -> Option<&'static String> {
    TARGET
        .get_or_init(|| {
            std::env::var("MEOWCAL_BAND_LOG")
                .ok()
                .filter(|p| !p.is_empty())
        })
        .as_ref()
}

/// Append one frame's geometry, if the instrument is switched on.
///
/// Best effort and deliberately silent on failure: this is scaffolding, and a
/// full disk or a bad path must not disturb a translation session that happens
/// to have it enabled.
pub fn record(lines: &[String], boxes: &[LineBox]) {
    let Some(path) = target() else {
        return;
    };
    if lines.is_empty() {
        return;
    }

    let elapsed = START.get_or_init(Instant::now).elapsed().as_millis();
    let mut entry = format!("{{\"kind\":\"line_geometry\",\"ms\":{elapsed},\"lines\":[");
    for (index, line) in lines.iter().enumerate() {
        if index > 0 {
            entry.push(',');
        }
        // A line with no geometry reports a zero rectangle rather than being
        // skipped, so the index stays aligned with what OCR returned.
        let area = boxes.get(index).copied().unwrap_or(LineBox {
            x: 0.0,
            y: 0.0,
            width: 0.0,
            height: 0.0,
        });
        entry.push_str(&format!(
            "{{\"x\":{:.0},\"y\":{:.0},\"w\":{:.0},\"h\":{:.0},\"chars\":{},\"digest\":\"{:016x}\"}}",
            area.x,
            area.y,
            area.width,
            area.height,
            line.chars().count(),
            digest(line)
        ));
    }
    entry.push_str("]}\n");

    if let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
    {
        let _ = file.write_all(entry.as_bytes());
    }
}

/// Identity without content. Two frames reading the same line agree; any
/// difference, including a single wrong glyph, disagrees - which is what makes
/// this usable as a "did this band change" signal despite OCR being unstable.
fn digest(line: &str) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    line.hash(&mut hasher);
    hasher.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_same_line_digests_the_same_and_a_changed_one_does_not() {
        assert_eq!(digest("先不提时钟塔"), digest("先不提时钟塔"));
        assert_ne!(digest("先不提时钟塔"), digest("先不提時鐘塔"));
    }

    // OCR is unstable enough that one glyph differs between reads of a static
    // subtitle. The digest must reflect that rather than smooth it over, or a
    // static band would look like it was changing.
    #[test]
    fn one_wrong_glyph_is_a_different_digest() {
        assert_ne!(digest("彳尔好"), digest("你好"));
    }

    #[test]
    fn the_instrument_is_off_without_the_environment_variable() {
        // Nothing is written and nothing panics when the target is unset,
        // which is how every ordinary session runs.
        record(&["anything".to_string()], &[]);
    }
}
