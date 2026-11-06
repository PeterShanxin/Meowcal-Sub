//! Text detection: input sizing and Differentiable Binarization post-processing.
//!
//! The constants are RapidOCR's PP-OCR defaults, except the detector side
//! limit, which caps the long side instead of raising the short one: raising a
//! subtitle strip's height to 736 makes it several thousand pixels wide.
//! Boxes are axis-aligned. Subtitles are horizontal, and an axis-aligned box
//! is what the reference minimum-area rectangle reduces to for them.

const LONG_SIDE_LIMIT: usize = 1440;
const BINARY_THRESHOLD: f32 = 0.3;
const BOX_THRESHOLD: f32 = 0.5;
const UNCLIP_RATIO: f32 = 1.6;
const MIN_SIDE: f32 = 3.0;
const MAX_CANDIDATES: usize = 1000;
const SAME_ROW_PIXELS: i32 = 10;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TextBox {
    pub x0: i32,
    pub y0: i32,
    pub x1: i32,
    pub y1: i32,
}

/// Detector input size for an image: the long side capped, both sides
/// rounded to the multiple of 32 the network requires. `None` when a side
/// rounds to zero.
pub fn input_size(width: usize, height: usize) -> Option<(usize, usize)> {
    let longest = width.max(height);
    let ratio = if longest > LONG_SIDE_LIMIT {
        LONG_SIDE_LIMIT as f64 / longest as f64
    } else {
        1.0
    };
    let side = |value: usize| round_to_32((value as f64 * ratio) as usize);
    let size = (side(width), side(height));
    (size.0 > 0 && size.1 > 0).then_some(size)
}

pub fn round_to_32(value: usize) -> usize {
    (value as f64 / 32.0).round_ties_even() as usize * 32
}

/// Boxes from a `height x width` probability map, scaled to a
/// `dest_width x dest_height` image and sorted into reading order.
pub fn boxes(
    probability: &[f32],
    width: usize,
    height: usize,
    dest_width: usize,
    dest_height: usize,
) -> Vec<TextBox> {
    let mask = dilate(probability, width, height);
    let mut found = Vec::new();
    for (x0, y0, x1, y1) in components(&mask, width, height)
        .into_iter()
        .take(MAX_CANDIDATES)
    {
        let (w, h) = ((x1 - x0) as f32, (y1 - y0) as f32);
        if w.min(h) < MIN_SIDE || mean(probability, width, x0, y0, x1, y1) < BOX_THRESHOLD {
            continue;
        }
        let grow = w * h * UNCLIP_RATIO / (2.0 * (w + h));
        if w.min(h) + 2.0 * grow < MIN_SIDE + 2.0 {
            continue;
        }
        let scale_x = |x: f32| (x / width as f32 * dest_width as f32).round_ties_even();
        let scale_y = |y: f32| (y / height as f32 * dest_height as f32).round_ties_even();
        let max_x = dest_width as f32 - 1.0;
        let max_y = dest_height as f32 - 1.0;
        let found_box = TextBox {
            x0: scale_x(x0 as f32 - grow).clamp(0.0, max_x) as i32,
            y0: scale_y(y0 as f32 - grow).clamp(0.0, max_y) as i32,
            x1: scale_x(x1 as f32 + grow).clamp(0.0, max_x) as i32,
            y1: scale_y(y1 as f32 + grow).clamp(0.0, max_y) as i32,
        };
        if found_box.x1 - found_box.x0 > 3 && found_box.y1 - found_box.y0 > 3 {
            found.push(found_box);
        }
    }
    reading_order(found)
}

/// Threshold, then dilate with OpenCV's 2x2 kernel anchored at its centre.
fn dilate(probability: &[f32], width: usize, height: usize) -> Vec<bool> {
    let on = |x: usize, y: usize| probability[y * width + x] > BINARY_THRESHOLD;
    let mut mask = vec![false; width * height];
    for y in 0..height {
        for x in 0..width {
            mask[y * width + x] = on(x, y)
                || (x > 0 && on(x - 1, y))
                || (y > 0 && on(x, y - 1))
                || (x > 0 && y > 0 && on(x - 1, y - 1));
        }
    }
    mask
}

/// Inclusive bounding boxes of 8-connected foreground regions, in scan order.
fn components(mask: &[bool], width: usize, height: usize) -> Vec<(usize, usize, usize, usize)> {
    let mut seen = vec![false; mask.len()];
    let mut regions = Vec::new();
    let mut stack = Vec::new();
    for start in 0..mask.len() {
        if !mask[start] || seen[start] {
            continue;
        }
        seen[start] = true;
        stack.push(start);
        let (mut x0, mut y0) = (start % width, start / width);
        let (mut x1, mut y1) = (x0, y0);
        while let Some(at) = stack.pop() {
            let (x, y) = (at % width, at / width);
            (x0, x1, y0, y1) = (x0.min(x), x1.max(x), y0.min(y), y1.max(y));
            for ny in y.saturating_sub(1)..=(y + 1).min(height - 1) {
                for nx in x.saturating_sub(1)..=(x + 1).min(width - 1) {
                    let next = ny * width + nx;
                    if mask[next] && !seen[next] {
                        seen[next] = true;
                        stack.push(next);
                    }
                }
            }
        }
        regions.push((x0, y0, x1, y1));
    }
    regions
}

fn mean(probability: &[f32], width: usize, x0: usize, y0: usize, x1: usize, y1: usize) -> f32 {
    let mut sum = 0.0;
    for y in y0..=y1 {
        sum += probability[y * width + x0..=y * width + x1]
            .iter()
            .sum::<f32>();
    }
    sum / ((x1 - x0 + 1) * (y1 - y0 + 1)) as f32
}

/// Top to bottom; boxes whose tops are within a few pixels of the previous
/// box share a row and read left to right.
fn reading_order(mut boxes: Vec<TextBox>) -> Vec<TextBox> {
    boxes.sort_by_key(|found| found.y0);
    let mut row = 0;
    let mut keyed: Vec<(usize, TextBox)> = Vec::with_capacity(boxes.len());
    for (index, found) in boxes.iter().enumerate() {
        if index > 0 && found.y0 - boxes[index - 1].y0 >= SAME_ROW_PIXELS {
            row += 1;
        }
        keyed.push((row, *found));
    }
    keyed.sort_by_key(|(row, found)| (*row, found.x0));
    keyed.into_iter().map(|(_, found)| found).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn map(width: usize, height: usize, rects: &[(usize, usize, usize, usize)]) -> Vec<f32> {
        let mut probability = vec![0.0; width * height];
        for &(x0, y0, x1, y1) in rects {
            for y in y0..=y1 {
                for x in x0..=x1 {
                    probability[y * width + x] = 0.9;
                }
            }
        }
        probability
    }

    #[test]
    fn input_size_caps_the_long_side_and_rounds_to_32() {
        assert_eq!(input_size(1110, 276), Some((1120, 288)));
        assert_eq!(input_size(2880, 400), Some((1440, 192)));
        assert_eq!(input_size(10, 10), None);
        // Python's round() rounds half to even: 48 / 32 = 1.5 -> 2, 80 / 32 = 2.5 -> 2.
        assert_eq!(round_to_32(48), 64);
        assert_eq!(round_to_32(80), 64);
    }

    #[test]
    fn a_text_region_becomes_one_grown_box_in_destination_pixels() {
        let probability = map(64, 32, &[(10, 10, 49, 19)]);
        let found = boxes(&probability, 64, 32, 128, 64);
        assert_eq!(found.len(), 1);
        let only = found[0];
        // Dilation adds a column and a row; unclipping grows every side.
        assert!(only.x0 < 20 && only.x1 > 98, "{only:?}");
        assert!(only.y0 < 20 && only.y1 > 38, "{only:?}");
    }

    #[test]
    fn weak_and_tiny_regions_are_dropped() {
        let mut probability = map(64, 32, &[(2, 2, 3, 3)]);
        for value in &mut probability[20 * 64 + 10..20 * 64 + 40] {
            *value = 0.35;
        }
        assert!(boxes(&probability, 64, 32, 64, 32).is_empty());
    }

    #[test]
    fn boxes_read_by_row_then_left_to_right() {
        let probability = map(
            200,
            80,
            &[(120, 12, 180, 22), (10, 14, 90, 24), (10, 50, 90, 60)],
        );
        let found = boxes(&probability, 200, 80, 200, 80);
        let lefts: Vec<i32> = found.iter().map(|found| found.x0).collect();
        assert_eq!(found.len(), 3);
        assert!(
            lefts[0] < lefts[1] && found[2].y0 > found[1].y0,
            "{found:?}"
        );
    }
}
