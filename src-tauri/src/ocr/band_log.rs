// =============================================================================
// BAND_LOG.RS - measurement scaffolding for subtitle band selection
// =============================================================================
// Temporary, and off unless asked for. Set MEOWCAL_BAND_LOG to a file path and
// every recognition appends one JSON line describing where text was found.
//
//   $env:MEOWCAL_BAND_LOG = "D:\tmp\bands.jsonl"
//
// The question it exists to answer: inside one tall capture region, can the
// subtitle be picked out from the page furniture around it by position and
// change alone? Grouping lines into bands is easy; choosing which band is the
// subtitle is the part that can be wrong, and guessing at it from first
// principles is how the last OCR hypothesis got shipped and turned out false.
//
// Recorded per line: its rectangle, how many characters it held, and a digest
// of the text. Never the text. `docs/evidence/README.md` excludes OCR source
// text from anything written to disk, and the digest answers the only question
// the analysis asks of it - did this band change since the last frame - without
// keeping what it said.
// =============================================================================

use super::LineBox;
use std::hash::{Hash, Hasher};
use std::io::Write;
use std::sync::OnceLock;
use std::time::Instant;

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
    let mut entry = format!("{{\"ms\":{elapsed},\"lines\":[");
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
