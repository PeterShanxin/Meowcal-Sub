// =============================================================================
// PREPROCESSING.RS - Image Preprocessing for OCR
// =============================================================================
// Pipeline: BGRA frame → grayscale → contrast stretch → adaptive binarize → BGRA
//
// The thresholding here has to survive a subtitle strip, which is an unusual
// image: overwhelmingly background, with a few percent of bright glyph pixels
// sitting on top of whatever the video happens to be showing. Any threshold
// derived from the pixel *population* rather than from the two intensity
// clusters promotes the brighter half of that background to the same value as
// the glyphs, and OCR is handed blocks instead of letters.
//
// Every stage after grayscale is a function of one pixel's intensity alone, so
// the whole chain collapses into a single 256-entry lookup table. That matters:
// a 3006x214 capture region is 643k pixels, and walking it once per stage - with
// an intermediate image allocated each time - cost 333ms per frame against the
// 19ms Windows OCR itself takes. Two passes and a table give the identical
// output; `fused_pipeline_matches_the_staged_reference` holds them to it.
// =============================================================================

use tracing::debug;

/// Configuration for image preprocessing
#[derive(Debug, Clone, Copy, Default)]
pub struct PreprocessingConfig {
    /// Convert image to grayscale
    pub grayscale: bool,
    /// Stretch the intensity range to use the full 0-255 span
    pub contrast_enhancement: bool,
    /// Split the image into glyphs and background at an automatically chosen
    /// threshold, then normalise to dark text on a light page.
    pub binarize: bool,
}

impl PreprocessingConfig {
    /// Create a new config with all options enabled (default for best OCR results)
    pub fn optimal() -> Self {
        Self {
            grayscale: true,
            contrast_enhancement: true,
            binarize: true,
        }
    }

    /// Check if any preprocessing is enabled
    pub fn is_enabled(&self) -> bool {
        self.grayscale || self.contrast_enhancement || self.binarize
    }
}

/// Apply preprocessing to BGRA image data for improved OCR results.
///
/// # Arguments
/// * `image_data` - Raw BGRA pixel data (4 bytes per pixel)
/// * `width` - Image width in pixels
/// * `height` - Image height in pixels
/// * `config` - Preprocessing configuration
///
/// # Returns
/// Preprocessed image data as BGRA bytes (same format as input)
pub fn preprocess_image(
    image_data: &[u8],
    width: u32,
    height: u32,
    config: PreprocessingConfig,
) -> Vec<u8> {
    if !config.is_enabled() {
        debug!("Preprocessing disabled, returning original image");
        return image_data.to_vec();
    }

    let expected_size = (width as usize) * (height as usize) * 4;
    if image_data.len() != expected_size {
        debug!("Invalid image size, returning original");
        return image_data.to_vec();
    }

    // Pass 1: intensity and its histogram together. Windows capture hands us
    // BGRA, so the channel order is B=0, G=1, R=2, A=3. Grayscale runs whether
    // or not the caller asked for it - every later stage is defined on
    // intensity alone.
    let mut intensity = vec![0u8; expected_size / 4];
    let mut histogram = [0u32; 256];
    for (gray, pixel) in intensity.iter_mut().zip(image_data.chunks_exact(4)) {
        let b = pixel[0] as f32;
        let g = pixel[1] as f32;
        let r = pixel[2] as f32;
        let luma = (0.299 * r + 0.587 * g + 0.114 * b) as u8;
        *gray = luma;
        histogram[luma as usize] += 1;
    }

    let lut = build_intensity_lut(&histogram, config);

    // Pass 2: write the shade straight into the BGRA the OCR engine wants.
    // Alpha is already opaque from the fill, so only three bytes move.
    let mut output = vec![255u8; expected_size];
    for (gray, pixel) in intensity.iter().zip(output.chunks_exact_mut(4)) {
        let shade = lut[*gray as usize];
        pixel[0] = shade;
        pixel[1] = shade;
        pixel[2] = shade;
    }

    debug!(
        width,
        height,
        contrast = config.contrast_enhancement,
        binarize = config.binarize,
        "Preprocessed {} bytes",
        output.len()
    );
    output
}

/// Collapse the contrast stretch and the adaptive threshold into one table
/// mapping source intensity to output shade.
///
/// Both stages are per-pixel functions of intensity, and the histogram already
/// carries everything either of them needs, so neither has to touch the image.
fn build_intensity_lut(histogram: &[u32; 256], config: PreprocessingConfig) -> [u8; 256] {
    let stretched = if config.contrast_enhancement {
        contrast_stretch_lut(histogram)
    } else {
        let mut identity = [0u8; 256];
        for (intensity, entry) in identity.iter_mut().enumerate() {
            *entry = intensity as u8;
        }
        identity
    };

    if !config.binarize {
        return stretched;
    }

    // The threshold has to be chosen in the *stretched* intensity space, which
    // is just the source histogram pushed through the stretch.
    let mut stretched_histogram = [0u32; 256];
    for (intensity, &count) in histogram.iter().enumerate() {
        stretched_histogram[stretched[intensity] as usize] += count;
    }

    let threshold = otsu_threshold(&stretched_histogram);
    let dark_pixels: u64 = stretched_histogram[..=threshold as usize]
        .iter()
        .map(|&count| count as u64)
        .sum();
    let total: u64 = stretched_histogram.iter().map(|&count| count as u64).sum();
    // Subtitles are light-on-dark and printed pages are dark-on-light. Rather
    // than guess which one arrived, treat the *smaller* cluster as the glyphs -
    // text never covers most of a capture region - and paint it black.
    let glyphs_are_dark = dark_pixels * 2 <= total;

    let mut binarized = [0u8; 256];
    for (intensity, entry) in binarized.iter_mut().enumerate() {
        let is_dark = stretched[intensity] <= threshold;
        *entry = if is_dark == glyphs_are_dark { 0 } else { 255 };
    }
    binarized
}

/// Linear contrast stretch as a lookup table.
///
/// A stretch, not histogram equalisation: on a subtitle strip equalisation
/// redistributes by pixel rank, which pulls the dominant background apart and
/// pushes its brighter half up into the glyph range. A stretch is monotonic, so
/// it never reorders intensities.
fn contrast_stretch_lut(histogram: &[u32; 256]) -> [u8; 256] {
    let min_val = histogram.iter().position(|&count| count > 0);
    let max_val = histogram.iter().rposition(|&count| count > 0);

    // A flat image has no range to stretch, and every pixel collapses to zero.
    let (min_val, max_val) = match (min_val, max_val) {
        (Some(min_val), Some(max_val)) if max_val > min_val => (min_val, max_val),
        _ => return [0u8; 256],
    };

    let mut lut = [0u8; 256];
    let range = (max_val - min_val) as f64;
    for (intensity, entry) in lut.iter_mut().enumerate() {
        let clamped = intensity.clamp(min_val, max_val);
        *entry = (((clamped - min_val) as f64) / range * 255.0).round() as u8;
    }
    lut
}

/// Otsu's method: the threshold that best separates the image into two
/// intensity clusters.
///
/// This is chosen over any fixed cut because it is driven by where the two
/// populations actually sit, not by how many pixels belong to each. A subtitle
/// strip that is 95% background and 5% glyphs splits correctly; a fixed
/// midpoint does not.
///
/// Returns the highest intensity that still belongs to the dark cluster.
fn otsu_threshold(histogram: &[u32; 256]) -> u8 {
    let total: u64 = histogram.iter().map(|&count| count as u64).sum();
    if total == 0 {
        return 127;
    }

    let weighted_total: u64 = histogram
        .iter()
        .enumerate()
        .map(|(intensity, &count)| intensity as u64 * count as u64)
        .sum();

    let mut background_weight: u64 = 0;
    let mut background_sum: u64 = 0;
    let mut best_threshold: u8 = 127;
    let mut best_variance = -1.0f64;

    for (intensity, &count) in histogram.iter().enumerate() {
        background_weight += count as u64;
        if background_weight == 0 {
            continue;
        }
        let foreground_weight = total - background_weight;
        if foreground_weight == 0 {
            break;
        }

        background_sum += intensity as u64 * count as u64;
        let background_mean = background_sum as f64 / background_weight as f64;
        let foreground_mean = (weighted_total - background_sum) as f64 / foreground_weight as f64;

        let spread = background_mean - foreground_mean;
        let variance = background_weight as f64 * foreground_weight as f64 * spread * spread;
        if variance > best_variance {
            best_variance = variance;
            best_threshold = intensity as u8;
        }
    }

    best_threshold
}

#[cfg(test)]
#[path = "preprocessing_tests.rs"]
mod tests;
