// =============================================================================
// LINE_GEOMETRY.RS - turning a WinRT recognition into lines and where they sat
// =============================================================================
// Windows OCR reports a bounding rectangle per *word*. A line's rectangle is
// their union, which nothing in this app computed before: the pipeline read
// `OcrLine::Text` and dropped the geometry on the floor.
//
// That is why a capture region taller than one subtitle could only ever be
// refused. Position is the signal that separates two possible subtitle
// positions from each other, and both from the page furniture between them.
// =============================================================================

use super::LineBox;
use windows::Media::Ocr::{OcrLine, OcrResult as WinRtOcrResult};

/// Extract the recognised lines and their rectangles, kept index-parallel.
///
/// A line whose geometry cannot be read still contributes its text with a zero
/// rectangle rather than being dropped: losing a subtitle because Windows would
/// not say where it was is a worse failure than not knowing where it was.
pub fn lines_with_boxes(result: &WinRtOcrResult) -> (Vec<String>, Vec<LineBox>) {
    let Ok(collection) = result.Lines() else {
        return (Vec::new(), Vec::new());
    };

    let mut lines = Vec::new();
    let mut boxes = Vec::new();

    for index in 0..collection.Size().unwrap_or(0) {
        let Ok(line) = collection.GetAt(index) else {
            continue;
        };
        let Ok(text) = line.Text() else {
            continue;
        };

        // `OcrLine::Text` joins the recognised words with a space, which for
        // Chinese and Japanese puts one between every glyph. See `text_cleanup`.
        let cleaned = super::text_cleanup::clean_line(&text.to_string());
        if cleaned.trim().is_empty() {
            continue;
        }

        lines.push(cleaned);
        boxes.push(bounds(&line).unwrap_or(LineBox {
            x: 0.0,
            y: 0.0,
            width: 0.0,
            height: 0.0,
        }));
    }

    // Off unless MEOWCAL_BAND_LOG is set. Recorded here because this is where
    // the geometry exists, which keeps the instrument out of the capture loop.
    super::band_log::record(&lines, &boxes);

    (lines, boxes)
}

/// The union of a line's word rectangles.
fn bounds(line: &OcrLine) -> Option<LineBox> {
    let words = line.Words().ok()?;

    let mut left = f32::MAX;
    let mut top = f32::MAX;
    let mut right = f32::MIN;
    let mut bottom = f32::MIN;
    let mut seen = false;

    for index in 0..words.Size().ok()? {
        let Ok(rect) = words.GetAt(index).and_then(|word| word.BoundingRect()) else {
            continue;
        };
        left = left.min(rect.X);
        top = top.min(rect.Y);
        right = right.max(rect.X + rect.Width);
        bottom = bottom.max(rect.Y + rect.Height);
        seen = true;
    }

    seen.then_some(LineBox {
        x: left,
        y: top,
        width: right - left,
        height: bottom - top,
    })
}

#[cfg(test)]
mod tests {
    use super::super::LineBox;

    #[test]
    fn a_line_is_grouped_by_where_its_middle_sits() {
        // Bands are formed from vertical centres, so a tall line and a short
        // one on the same baseline have to land near each other.
        let tall = LineBox {
            x: 0.0,
            y: 100.0,
            width: 50.0,
            height: 40.0,
        };
        let short = LineBox {
            x: 60.0,
            y: 110.0,
            width: 50.0,
            height: 20.0,
        };
        assert_eq!(tall.middle_y(), 120.0);
        assert_eq!(short.middle_y(), 120.0);
    }
}
