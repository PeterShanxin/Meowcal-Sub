// =============================================================================
// BAND_GEOMETRY.RS - turning a frame's rectangles into one band's observation
// =============================================================================
// Small and separate because it is the only arithmetic in band selection that
// has nothing to do with time: given the boxes that landed in one band this
// frame, where is that band, and how close does another line have to be to
// belong to it.
// =============================================================================

use super::LineBox;

/// How close two vertical centres must be to count as the same band, as a
/// multiple of the typical line height in the frame.
///
/// Relative to line height rather than absolute so it holds at any resolution.
const BAND_TOLERANCE: f32 = 0.75;

/// Fallback line height when a frame reports no usable geometry, so grouping
/// still has a tolerance to work with rather than putting every line in a band
/// of its own.
const ASSUMED_LINE_HEIGHT: f32 = 32.0;

/// The vertical distance within which two lines belong to the same band.
///
/// Taken from the median line height in this frame, so a subtitle rendered
/// large and a caption rendered small each get a tolerance suited to them.
pub(super) fn tolerance(boxes: &[LineBox]) -> f32 {
    let mut heights: Vec<f32> = boxes
        .iter()
        .map(|area| area.height)
        .filter(|height| *height > 0.0)
        .collect();
    if heights.is_empty() {
        return BAND_TOLERANCE * ASSUMED_LINE_HEIGHT;
    }
    heights.sort_by(f32::total_cmp);
    BAND_TOLERANCE * heights[heights.len() / 2]
}

/// The horizontal span and vertical centre of the lines that share a band this
/// frame, as `(left, right, centre_y)`.
///
/// The span is the union, because a band's position is where its text starts
/// and ends, not where any one line does. The centre is the mean, so a
/// two-line subtitle is placed between its lines rather than on one of them.
pub(super) fn union(boxes: &[LineBox], lines: &[usize]) -> (f32, f32, f32) {
    let left = lines
        .iter()
        .map(|index| boxes[*index].x)
        .fold(f32::MAX, f32::min);
    let right = lines
        .iter()
        .map(|index| boxes[*index].x + boxes[*index].width)
        .fold(f32::MIN, f32::max);
    let centre_y = lines
        .iter()
        .map(|index| boxes[*index].middle_y())
        .sum::<f32>()
        / lines.len().max(1) as f32;
    (left, right, centre_y)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn area(x: f32, y: f32, width: f32, height: f32) -> LineBox {
        LineBox {
            x,
            y,
            width,
            height,
        }
    }

    #[test]
    fn the_tolerance_follows_the_typical_line_height() {
        let small = [area(0.0, 0.0, 100.0, 20.0); 3];
        let large = [area(0.0, 0.0, 100.0, 60.0); 3];
        assert!(tolerance(&small) < tolerance(&large));
        assert_eq!(tolerance(&large), 45.0);
    }

    // One freak line height must not drag the tolerance for the whole frame,
    // which is why this is a median and not a mean.
    #[test]
    fn one_outsized_line_does_not_set_the_tolerance() {
        let boxes = [
            area(0.0, 0.0, 100.0, 40.0),
            area(0.0, 0.0, 100.0, 40.0),
            area(0.0, 0.0, 100.0, 400.0),
        ];
        assert_eq!(tolerance(&boxes), 30.0);
    }

    // A frame where Windows reported no geometry still needs a tolerance, or
    // every line becomes its own band and nothing is ever established.
    #[test]
    fn a_frame_without_geometry_still_has_a_tolerance() {
        assert!(tolerance(&[]) > 0.0);
        assert!(tolerance(&[area(0.0, 0.0, 0.0, 0.0)]) > 0.0);
    }

    #[test]
    fn the_span_is_the_union_of_the_lines_that_share_the_band() {
        let boxes = [
            area(100.0, 1000.0, 200.0, 40.0),
            area(400.0, 1000.0, 150.0, 40.0),
        ];
        let (left, right, _) = union(&boxes, &[0, 1]);
        assert_eq!((left, right), (100.0, 550.0));
    }

    #[test]
    fn the_centre_sits_between_the_lines_of_a_wrapped_subtitle() {
        let boxes = [
            area(100.0, 1000.0, 200.0, 40.0),
            area(100.0, 1060.0, 200.0, 40.0),
        ];
        let (_, _, centre_y) = union(&boxes, &[0, 1]);
        assert_eq!(centre_y, 1050.0);
    }

    #[test]
    fn a_single_line_is_its_own_span() {
        let boxes = [area(100.0, 1000.0, 200.0, 40.0)];
        assert_eq!(union(&boxes, &[0]), (100.0, 300.0, 1020.0));
    }
}
