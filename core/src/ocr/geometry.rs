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

        let text = text.to_string();
        if text.trim().is_empty() {
            continue;
        }

        lines.push(text);
        boxes.push(bounds(&line).unwrap_or(LineBox {
            x: 0.0,
            y: 0.0,
            width: 0.0,
            height: 0.0,
        }));
    }

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
