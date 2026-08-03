// =============================================================================
// FRAME_BUDGET.RS - Keeping one capture from taking the app down with it
// =============================================================================
// A subtitle strip is a few hundred thousand pixels. A capture region dragged
// across the whole screen is twenty million bytes, and every frame is copied
// several times on its way through recognition: the preprocessed buffer, the
// IBuffer handed to WinRT, and the SoftwareBitmap built from it. At four frames
// a second that is a sustained allocation rate the process cannot hold.
//
// It does not fail cleanly. Windows OCR starts returning 0x8007000E, "not
// enough memory resources are available", which the pipeline logs and retries
// against the same oversized frame forever - and the WebView2 renderer, sharing
// that memory, is killed and repaints white. The app looks crashed while the
// Rust side is still translating happily, which is exactly what a 2944x1712
// region produced in a live session.
//
// So a frame is scaled to fit a pixel budget before any of that copying starts.
// A real subtitle strip is far below the budget and passes through untouched;
// only a region big enough to threaten the process is touched at all, and for
// that region a smaller image read late beats no image read at all.
// =============================================================================

/// Pixels a single frame may carry into recognition.
///
/// At four bytes per pixel this caps one frame's BGRA buffer at 8MB, and the
/// handful of copies recognition makes of it at roughly 32MB. A 2920x176
/// subtitle strip is 514k pixels, a quarter of the budget, so the sizes this
/// app is built for never reach the scaler.
const MAX_FRAME_PIXELS: u32 = 2_000_000;

/// How much to shrink a frame so it fits the budget, or `None` if it already
/// does.
///
/// Returned as target dimensions rather than a ratio so the caller is never the
/// one rounding: a width and height that disagree with the buffer length by a
/// pixel is an invalid image, and recognition rejects it.
pub fn fit_to_budget(width: u32, height: u32) -> Option<(u32, u32)> {
    let pixels = u64::from(width) * u64::from(height);
    if pixels <= u64::from(MAX_FRAME_PIXELS) || width == 0 || height == 0 {
        return None;
    }

    let ratio = (f64::from(MAX_FRAME_PIXELS) / pixels as f64).sqrt();
    // Both sides floor to stay under budget, and both floor to at least one
    // pixel so a very long thin region cannot round itself out of existence.
    let target_width = ((f64::from(width) * ratio).floor() as u32).max(1);
    let target_height = ((f64::from(height) * ratio).floor() as u32).max(1);

    if target_width >= width && target_height >= height {
        return None;
    }
    Some((target_width, target_height))
}

/// Scale a BGRA frame down by averaging each target pixel over the source
/// pixels it covers.
///
/// Point sampling was tried first and is wrong here. Dropping pixels aliases
/// against the strokes of a glyph, and the damage is not monotonic in the scale
/// factor: measured against a real frame, sampling at 0.75 read *worse* than
/// either 1.0 or 0.5, because at that ratio the pixels being dropped happened
/// to be the ones carrying the strokes. Averaging degrades smoothly, which is
/// the property a scale factor chosen at runtime needs.
pub fn scale_bgra(
    image_data: &[u8],
    width: u32,
    height: u32,
    target_width: u32,
    target_height: u32,
) -> Option<Vec<u8>> {
    let expected = (width as usize)
        .checked_mul(height as usize)?
        .checked_mul(4)?;
    if image_data.len() != expected || target_width == 0 || target_height == 0 {
        return None;
    }
    if target_width > width || target_height > height {
        return None;
    }

    let (width, height) = (width as usize, height as usize);
    let (target_width, target_height) = (target_width as usize, target_height as usize);
    let mut scaled = vec![0u8; target_width * target_height * 4];

    for y in 0..target_height {
        let source_top = y * height / target_height;
        let source_bottom = (((y + 1) * height).div_ceil(target_height)).min(height);
        let source_bottom = source_bottom.max(source_top + 1);

        for x in 0..target_width {
            let source_left = x * width / target_width;
            let source_right = (((x + 1) * width).div_ceil(target_width)).min(width);
            let source_right = source_right.max(source_left + 1);

            let mut totals = [0u32; 4];
            let mut counted = 0u32;
            for source_y in source_top..source_bottom {
                let row = source_y * width * 4;
                for source_x in source_left..source_right {
                    let pixel = row + source_x * 4;
                    for channel in 0..4 {
                        totals[channel] += u32::from(image_data[pixel + channel]);
                    }
                    counted += 1;
                }
            }

            let target = (y * target_width + x) * 4;
            for channel in 0..4 {
                scaled[target + channel] = (totals[channel] / counted) as u8;
            }
        }
    }
    Some(scaled)
}

/// Effective resolution, in logical pixels per logical pixel, that recognition
/// is given.
///
/// A 2x display renders a subtitle at roughly 60 pixels of glyph height; a 1x
/// display renders the same subtitle at 30, and Windows OCR reads both. Below
/// about 20 it stops: measured over three subtitle runs, accuracy held from
/// full resolution down to 0.4 and collapsed at 0.3, which is where those 60px
/// glyphs cross 20. Normalising to 1x therefore costs nothing that was
/// measurable and takes a quarter of the memory and preprocessing work on a
/// high-DPI display, while leaving a 1x display alone entirely.
///
/// Slightly above 1.0 rather than exactly, so a display at a fractional scale
/// like 1.25 is not shaved for no reason.
const TARGET_LOGICAL_SCALE: f64 = 1.25;

/// Both reductions in the order they have to happen, borrowing the frame
/// unchanged when neither applies.
///
/// DPI first, because it is the cheap one and usually enough: a 2x full-screen
/// capture normalised to one logical pixel already lands under the budget, so
/// the safety valve never fires in the case that produced it. `capture_scale`
/// is the display scale the region was captured at; 1.0 disables the first
/// step entirely.
pub fn fit_frame(
    image_data: &[u8],
    width: u32,
    height: u32,
    capture_scale: f64,
) -> (std::borrow::Cow<'_, [u8]>, u32, u32) {
    let dpi_target = (capture_scale.is_finite() && capture_scale > TARGET_LOGICAL_SCALE)
        .then(|| {
            let ratio = TARGET_LOGICAL_SCALE / capture_scale;
            let target_width = ((f64::from(width) * ratio).floor() as u32).max(1);
            let target_height = ((f64::from(height) * ratio).floor() as u32).max(1);
            (target_width < width && target_height < height)
                .then_some((target_width, target_height))
        })
        .flatten();

    let (width, height, dpi_scaled) = match dpi_target {
        Some((target_width, target_height)) => {
            match scale_bgra(image_data, width, height, target_width, target_height) {
                Some(scaled) => (target_width, target_height, Some(scaled)),
                None => (width, height, None),
            }
        }
        None => (width, height, None),
    };

    let borrowed = |data: Option<Vec<u8>>| match data {
        Some(scaled) => std::borrow::Cow::Owned(scaled),
        None => std::borrow::Cow::Borrowed(image_data),
    };

    let Some((target_width, target_height)) = fit_to_budget(width, height) else {
        return (borrowed(dpi_scaled), width, height);
    };
    let source = dpi_scaled.as_deref().unwrap_or(image_data);
    let Some(scaled) = scale_bgra(source, width, height, target_width, target_height) else {
        // A buffer that disagrees with its dimensions is recognition's to
        // reject, with the size it actually arrived at.
        return (borrowed(dpi_scaled), width, height);
    };

    if LAST_SCALED.swap(
        u64::from(width) << 32 | u64::from(height),
        std::sync::atomic::Ordering::Relaxed,
    ) != u64::from(width) << 32 | u64::from(height)
    {
        tracing::info!(
            "Capture region is {}x{} ({} MB per frame); scaling to {}x{} to keep recognition \
             inside its memory budget.",
            width,
            height,
            (u64::from(width) * u64::from(height) * 4) / 1_048_576,
            target_width,
            target_height
        );
    }

    (std::borrow::Cow::Owned(scaled), target_width, target_height)
}

static LAST_SCALED: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

#[cfg(test)]
mod tests {
    use super::*;

    // The shape this app is built for. Touching it would cost accuracy for
    // nothing.
    #[test]
    fn a_subtitle_strip_is_left_alone() {
        assert_eq!(fit_to_budget(2920, 176), None);
        assert_eq!(fit_to_budget(3012, 250), None);
        assert_eq!(fit_to_budget(1920, 1040), None);
    }

    // The region from the session that white-screened the app.
    #[test]
    fn a_full_screen_region_is_brought_under_budget() {
        let (width, height) = fit_to_budget(2944, 1712).expect("scaled");
        assert!(
            u64::from(width) * u64::from(height) <= u64::from(MAX_FRAME_PIXELS),
            "{width}x{height} still over budget"
        );
        // Still recognisable proportions, not a thumbnail.
        assert!(width > 1500, "{width}");
        let aspect_before = 2944.0 / 1712.0;
        let aspect_after = f64::from(width) / f64::from(height);
        assert!((aspect_before - aspect_after).abs() < 0.01);
    }

    #[test]
    fn a_very_wide_region_keeps_at_least_one_row() {
        let (width, height) = fit_to_budget(60_000, 400).expect("scaled");
        assert!(height >= 1);
        assert!(u64::from(width) * u64::from(height) <= u64::from(MAX_FRAME_PIXELS));
    }

    #[test]
    fn an_empty_frame_has_nothing_to_scale() {
        assert_eq!(fit_to_budget(0, 100), None);
        assert_eq!(fit_to_budget(100, 0), None);
    }

    #[test]
    fn scaling_produces_exactly_the_buffer_recognition_expects() {
        let source = vec![7u8; 40 * 20 * 4];
        let scaled = scale_bgra(&source, 40, 20, 10, 5).expect("scaled");
        assert_eq!(scaled.len(), 10 * 5 * 4);
        assert!(scaled.iter().all(|byte| *byte == 7));
    }

    // A buffer that does not match its stated size is the one thing that must
    // not be scaled: the result would be silently wrong rather than rejected.
    #[test]
    fn a_mismatched_buffer_is_refused() {
        let source = vec![0u8; 10];
        assert!(scale_bgra(&source, 40, 20, 10, 5).is_none());
    }

    #[test]
    fn each_target_pixel_averages_the_source_pixels_it_covers() {
        // Four pixels, values 0/10/20/30, halved: each output averages a pair.
        let mut source = vec![0u8; 4 * 4];
        for (index, value) in [0u8, 10, 20, 30].iter().enumerate() {
            source[index * 4..index * 4 + 4].copy_from_slice(&[*value; 4]);
        }
        let scaled = scale_bgra(&source, 4, 1, 2, 1).expect("scaled");
        assert_eq!(scaled, vec![5, 5, 5, 5, 25, 25, 25, 25]);
    }

    // Averaging must not blow past a channel's range on a saturated frame.
    #[test]
    fn a_fully_white_frame_stays_white() {
        let source = vec![255u8; 8 * 8 * 4];
        let scaled = scale_bgra(&source, 8, 8, 3, 3).expect("scaled");
        assert!(scaled.iter().all(|byte| *byte == 255));
    }

    // Upscaling is not this function's job, and asking for it would read source
    // pixels that do not exist.
    #[test]
    fn upscaling_is_refused() {
        let source = vec![0u8; 4 * 4];
        assert!(scale_bgra(&source, 4, 1, 8, 1).is_none());
    }

    // A 1x display is already at the resolution recognition wants; touching it
    // would cost accuracy for nothing, and must not even copy the buffer.
    #[test]
    fn a_standard_dpi_capture_is_passed_through_untouched() {
        let source = vec![9u8; 40 * 10 * 4];
        for scale in [1.0, 1.25, 0.0, f64::NAN] {
            let (data, width, height) = fit_frame(&source, 40, 10, scale);
            assert!(matches!(data, std::borrow::Cow::Borrowed(_)), "{scale}");
            assert_eq!((width, height), (40, 10));
        }
    }

    // The measured case: a 2x subtitle strip halves, which is a quarter of the
    // pixels through preprocessing and every WinRT copy.
    #[test]
    fn a_high_dpi_capture_is_reduced_toward_one_logical_pixel() {
        let source = vec![9u8; 2920 * 176 * 4];
        let (data, width, height) = fit_frame(&source, 2920, 176, 2.0);
        assert_eq!((width, height), (1825, 110));
        assert_eq!(data.len(), 1825 * 110 * 4);
        assert!(width * height * 4 < 2920 * 176 * 4 / 2);
    }

    // The region that white-screened the app, put through the DPI rule the live
    // pipeline applies first. Reducing a 2x full-screen capture toward one
    // logical pixel is on its own enough to bring it under the safety budget,
    // so the emergency scaler never has to fire in the case that produced it.
    #[test]
    fn a_full_screen_capture_on_a_2x_display_lands_under_budget_from_dpi_alone() {
        let (_, width, height) = {
            let source = vec![0u8; 2944 * 1712 * 4];
            let (data, width, height) = fit_frame(&source, 2944, 1712, 2.0);
            (data.len(), width, height)
        };
        assert!(
            u64::from(width) * u64::from(height) <= u64::from(MAX_FRAME_PIXELS),
            "{width}x{height} still over budget"
        );
        assert_eq!(fit_to_budget(width, height), None);
    }
}
