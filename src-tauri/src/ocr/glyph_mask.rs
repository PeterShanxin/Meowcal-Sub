// =============================================================================
// GLYPH_MASK.RS - Reading white subtitle glyphs by colour, not brightness
// =============================================================================
// Burned-in subtitles are white fill with a dark outline. Over a bright scene -
// a yellow wall, a green field - the fill and the background are equally
// bright, so the luminance threshold in `preprocessing` puts them in one class
// and paints only the outline: Windows OCR receives hollow glyphs and returns
// their radicals (#53). The raw frame fares no better, because the outline and
// the scene clutter read as strokes.
//
// Colour separates them. The fill is bright *and* unsaturated; a bright scene
// is saturated. Painting exactly those pixels black on white hands OCR solid
// glyphs on a clean page.
//
// The mask also adds a white margin. A region drawn tight to the line leaves
// glyphs touching the frame edge, and Windows OCR splits those into radicals
// even when the mask is clean.
// =============================================================================

use super::{LineBox, OcrResult};

/// Lowest channel value a glyph pixel may have. Anti-aliased stroke edges fall
/// below it and are dropped, which thins strokes slightly but keeps them apart.
const MIN_CHANNEL: u8 = 200;

/// Largest spread between a glyph pixel's channels. Pale yellow scene pixels
/// can pass `MIN_CHANNEL`; their blue channel keeps them out here.
const MAX_SPREAD: u8 = 40;

/// White border added on every side, in frame pixels, where Core's dimension
/// limit leaves room for it.
const MARGIN: u32 = 24;

const GLYPH: u8 = 0;
const PAPER: u8 = 255;

pub struct MaskedFrame {
    pub bgra: Vec<u8>,
    pub width: u32,
    pub height: u32,
    pub margin: u32,
}

/// Paint bright, unsaturated pixels black on white and add the margin.
///
/// The caller must pass the recognised geometry through [`remove_margin`]
/// with the frame's `margin` before using it.
pub fn mask_white_glyphs(bgra: &[u8], width: u32, height: u32) -> MaskedFrame {
    let room = meowcal_core::ocr::MAX_DIMENSION.saturating_sub(width.max(height));
    let margin = MARGIN.min(room / 2);
    let padded_width = width + 2 * margin;
    let padded_height = height + 2 * margin;
    let mut out = vec![PAPER; (padded_width as usize) * (padded_height as usize) * 4];
    let row_bytes = width as usize * 4;
    for (row, source) in bgra.chunks_exact(row_bytes).enumerate() {
        let start = ((row + margin as usize) * padded_width as usize + margin as usize) * 4;
        let target = &mut out[start..start + row_bytes];
        for (pixel, dest) in source
            .as_chunks::<4>()
            .0
            .iter()
            .zip(target.as_chunks_mut::<4>().0)
        {
            let (b, g, r) = (pixel[0], pixel[1], pixel[2]);
            let low = b.min(g).min(r);
            let high = b.max(g).max(r);
            let value = if low >= MIN_CHANNEL && high - low <= MAX_SPREAD {
                GLYPH
            } else {
                PAPER
            };
            dest[..3].fill(value);
        }
    }
    MaskedFrame {
        bgra: out,
        width: padded_width,
        height: padded_height,
        margin,
    }
}

/// Express a result recognised on a masked frame in the unpadded frame's
/// pixels, so band selection sees the geometry it would have seen unmasked.
pub fn remove_margin(result: OcrResult, width: u32, margin: u32) -> OcrResult {
    let margin = margin as f32;
    let boxes = result
        .boxes
        .iter()
        .map(|b| LineBox {
            x: b.x - margin,
            y: b.y - margin,
            ..*b
        })
        .collect();
    OcrResult::with_boxes(result.lines, boxes, width as f32)
}

#[cfg(test)]
mod tests {
    use super::*;

    const WHITE: [u8; 4] = [255, 255, 255, 255];
    const OUTLINE: [u8; 4] = [30, 30, 30, 255];
    // BGRA for a bright yellow scene, the case the luminance threshold hollows.
    const YELLOW: [u8; 4] = [90, 225, 240, 255];

    fn pixel(frame: &[u8], width: u32, x: u32, y: u32) -> u8 {
        frame[((y * width + x) * 4) as usize]
    }

    /// A 5x5 white glyph with a one-pixel dark outline, centred in a 9x9
    /// yellow frame.
    fn outlined_glyph_on_yellow() -> Vec<u8> {
        let mut frame = Vec::new();
        for y in 0..9 {
            for x in 0..9 {
                let inside = |lo, hi| (lo..=hi).contains(&x) && (lo..=hi).contains(&y);
                frame.extend(if inside(3, 5) {
                    WHITE
                } else if inside(2, 6) {
                    OUTLINE
                } else {
                    YELLOW
                });
            }
        }
        frame
    }

    #[test]
    fn a_white_glyph_on_a_bright_scene_stays_solid() {
        let masked = mask_white_glyphs(&outlined_glyph_on_yellow(), 9, 9);
        let at = |x, y| pixel(&masked.bgra, masked.width, x + MARGIN, y + MARGIN);
        assert_eq!(at(4, 4), GLYPH, "glyph fill");
        assert_eq!(at(2, 2), PAPER, "outline");
        assert_eq!(at(0, 0), PAPER, "bright yellow scene");
    }

    #[test]
    fn the_margin_is_blank_paper() {
        let masked = mask_white_glyphs(&[255; 4], 1, 1);
        let (width, height) = (masked.width, masked.height);
        assert_eq!((width, height), (1 + 2 * MARGIN, 1 + 2 * MARGIN));
        assert_eq!(pixel(&masked.bgra, width, MARGIN, MARGIN), GLYPH);
        let blank = masked
            .bgra
            .as_chunks::<4>()
            .0
            .iter()
            .filter(|p| p[..3] == [PAPER; 3])
            .count();
        assert_eq!(blank, (width * height - 1) as usize);
    }

    #[test]
    fn the_margin_never_takes_a_frame_past_cores_limit() {
        let limit = meowcal_core::ocr::MAX_DIMENSION;
        let masked = mask_white_glyphs(&vec![0; (limit as usize - 6) * 4], limit - 6, 1);
        assert_eq!((masked.width, masked.height), (limit, 7));
        assert_eq!(
            mask_white_glyphs(&vec![0; limit as usize * 4], limit, 1).width,
            limit
        );
    }

    #[test]
    fn geometry_is_reported_in_unpadded_pixels() {
        let masked = mask_white_glyphs(&vec![0; 500 * 4], 500, 1);
        let padded = OcrResult::with_boxes(
            vec!["line".into()],
            vec![LineBox {
                x: 30.0,
                y: 40.0,
                width: 100.0,
                height: 20.0,
            }],
            (500 + 2 * MARGIN) as f32,
        );
        let restored = remove_margin(padded, 500, masked.margin);
        assert_eq!(restored.frame_width, 500.0);
        let b = restored.boxes[0];
        assert_eq!(
            (b.x, b.y, b.width, b.height),
            (30.0 - MARGIN as f32, 40.0 - MARGIN as f32, 100.0, 20.0)
        );
        assert_eq!(restored.text, "line");
    }
}
