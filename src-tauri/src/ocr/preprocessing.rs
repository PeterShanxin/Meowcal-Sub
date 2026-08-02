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
// =============================================================================

use image::{GrayImage, ImageBuffer, Luma, Rgba};
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

    debug!(
        "Preprocessing image: {}x{}, grayscale: {}, contrast: {}, binarize: {}",
        width, height, config.grayscale, config.contrast_enhancement, config.binarize
    );

    let expected_size = (width * height * 4) as usize;
    if image_data.len() != expected_size {
        debug!("Invalid image size, returning original");
        return image_data.to_vec();
    }

    let rgba_image =
        match ImageBuffer::<Rgba<u8>, Vec<u8>>::from_raw(width, height, image_data.to_vec()) {
            Some(img) => img,
            None => {
                debug!("Failed to create image buffer, returning original");
                return image_data.to_vec();
            }
        };

    // Step 1: grayscale. Windows capture hands us BGRA, so the channel order is
    // B=0, G=1, R=2, A=3. This runs whether or not the caller asked for it -
    // every later step is defined on intensity alone.
    let mut gray_image = GrayImage::new(width, height);
    for (x, y, pixel) in rgba_image.enumerate_pixels() {
        let b = pixel[0] as f32;
        let g = pixel[1] as f32;
        let r = pixel[2] as f32;
        gray_image.put_pixel(x, y, Luma([(0.299 * r + 0.587 * g + 0.114 * b) as u8]));
    }

    // Step 2: contrast. A linear stretch, not histogram equalisation: on a
    // subtitle strip equalisation redistributes by pixel rank, which pulls the
    // dominant background apart and pushes its brighter half up into the glyph
    // range. A stretch is monotonic, so it never reorders intensities.
    let stretched: GrayImage = if config.contrast_enhancement {
        debug!("Applying contrast stretch...");
        apply_contrast_stretch(&gray_image)
    } else {
        debug!("Skipping contrast enhancement");
        gray_image
    };

    let final_gray: GrayImage = if config.binarize {
        debug!("Applying adaptive threshold...");
        apply_adaptive_binarize(&stretched)
    } else {
        debug!("Skipping binarization");
        stretched
    };

    // Convert grayscale back to BGRA for OCR (Windows OCR expects BGRA)
    let mut output = Vec::with_capacity(expected_size);
    for pixel in final_gray.pixels() {
        let gray_value = pixel[0];
        output.push(gray_value); // B
        output.push(gray_value); // G
        output.push(gray_value); // R
        output.push(255); // A
    }

    debug!("Preprocessing complete: {} bytes output", output.len());
    output
}

/// Apply linear contrast stretch to enhance image contrast.
///
/// Stretches the intensity range to use the full 0-255 range.
pub fn apply_contrast_stretch(image: &GrayImage) -> GrayImage {
    let (width, height) = image.dimensions();

    let mut min_val: u8 = 255;
    let mut max_val: u8 = 0;
    for pixel in image.pixels() {
        let val = pixel[0];
        min_val = min_val.min(val);
        max_val = max_val.max(val);
    }

    // If all pixels are the same value there is no range to stretch — return all zeros
    if max_val == min_val {
        return GrayImage::new(width, height);
    }

    let mut output = GrayImage::new(width, height);
    let range = (max_val - min_val) as f64;
    for (x, y, pixel) in image.enumerate_pixels() {
        let stretched = ((pixel[0] as f64 - min_val as f64) / range * 255.0).round() as u8;
        output.put_pixel(x, y, Luma([stretched]));
    }

    output
}

/// Build the 256-bucket intensity histogram of a grayscale image.
fn histogram_of(image: &GrayImage) -> [u32; 256] {
    let mut histogram = [0u32; 256];
    for pixel in image.pixels() {
        histogram[pixel[0] as usize] += 1;
    }
    histogram
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

/// Split an image into glyphs and background, then normalise to dark text on a
/// light page.
///
/// Subtitles are light-on-dark and printed pages are dark-on-light. Rather than
/// guess which one arrived, this treats the *smaller* cluster as the glyphs -
/// text never covers most of a capture region - and paints it black.
fn apply_adaptive_binarize(image: &GrayImage) -> GrayImage {
    let histogram = histogram_of(image);
    let threshold = otsu_threshold(&histogram);

    let dark_pixels: u64 = histogram[..=threshold as usize]
        .iter()
        .map(|&count| count as u64)
        .sum();
    let total: u64 = histogram.iter().map(|&count| count as u64).sum();
    let glyphs_are_dark = dark_pixels * 2 <= total;

    let (width, height) = image.dimensions();
    let mut output = GrayImage::new(width, height);
    for (x, y, pixel) in image.enumerate_pixels() {
        let is_dark = pixel[0] <= threshold;
        let is_glyph = is_dark == glyphs_are_dark;
        output.put_pixel(x, y, Luma([if is_glyph { 0 } else { 255 }]));
    }
    output
}

#[cfg(test)]
#[path = "preprocessing_tests.rs"]
mod tests;
