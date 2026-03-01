// =============================================================================
// PREPROCESSING.RS - Image Preprocessing for OCR
// =============================================================================
// This module provides image preprocessing functions to improve OCR quality.
// Pipeline: BGRA Image → Grayscale → Contrast Enhancement → Binarize → Output
// =============================================================================

use image::{GrayImage, ImageBuffer, Luma, Rgba};
use tracing::debug;

/// Configuration for image preprocessing
#[derive(Debug, Clone, Copy, Default)]
pub struct PreprocessingConfig {
    /// Convert image to grayscale
    pub grayscale: bool,
    /// Apply contrast enhancement (histogram equalization)
    pub contrast_enhancement: bool,
    /// Apply binary threshold after contrast enhancement.
    /// Converts image to pure black and white at the midpoint (128/255).
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

    // Create RGBA image from raw bytes
    let rgba_image =
        match ImageBuffer::<Rgba<u8>, Vec<u8>>::from_raw(width, height, image_data.to_vec()) {
            Some(img) => img,
            None => {
                debug!("Failed to create image buffer, returning original");
                return image_data.to_vec();
            }
        };

    // Step 1: Convert to grayscale if enabled
    let gray_image: GrayImage = if config.grayscale {
        debug!("Converting to grayscale...");
        // Manual grayscale conversion using luminance formula: 0.299*R + 0.587*G + 0.114*B
        let mut gray = GrayImage::new(width, height);
        for (x, y, pixel) in rgba_image.enumerate_pixels() {
            let r = pixel[0] as f32;
            let g = pixel[1] as f32;
            let b = pixel[2] as f32;
            let gray_val = (0.299 * r + 0.587 * g + 0.114 * b) as u8;
            gray.put_pixel(x, y, Luma([gray_val]));
        }
        gray
    } else {
        // Convert RGBA to grayscale using same formula
        debug!("Converting to grayscale for OCR...");
        let mut gray = GrayImage::new(width, height);
        for (x, y, pixel) in rgba_image.enumerate_pixels() {
            let r = pixel[0] as f32;
            let g = pixel[1] as f32;
            let b = pixel[2] as f32;
            let gray_val = (0.299 * r + 0.587 * g + 0.114 * b) as u8;
            gray.put_pixel(x, y, Luma([gray_val]));
        }
        gray
    };

    // Step 2: Apply contrast enhancement if enabled
    let after_eq: GrayImage = if config.contrast_enhancement {
        debug!("Applying contrast enhancement...");
        apply_histogram_equalization(&gray_image)
    } else {
        debug!("Skipping contrast enhancement");
        gray_image
    };

    // Step 3: Apply binary threshold if enabled
    // Pixels below 128 become 0 (black); 128 and above become 255 (white).
    // Applied after EQ so the threshold is always at the normalized midpoint.
    let final_gray: GrayImage = if config.binarize {
        debug!("Applying binary threshold (128)...");
        apply_binarize(&after_eq)
    } else {
        debug!("Skipping binarization");
        after_eq
    };

    // Convert grayscale back to BGRA for OCR (Windows OCR expects BGRA)
    let mut output = Vec::with_capacity(expected_size);
    for pixel in final_gray.pixels() {
        let gray_value = pixel[0];
        // BGRA format: Blue, Green, Red, Alpha
        output.push(gray_value); // B
        output.push(gray_value); // G
        output.push(gray_value); // R
        output.push(255); // A (fully opaque)
    }

    debug!("Preprocessing complete: {} bytes output", output.len());
    output
}

/// Apply histogram equalization to enhance image contrast.
///
/// This technique spreads out the intensity distribution, making dark
/// text more distinguishable from light backgrounds.
fn apply_histogram_equalization(image: &GrayImage) -> GrayImage {
    let (width, height) = image.dimensions();
    let total_pixels = width * height;

    // Build histogram (256 buckets for 8-bit grayscale)
    let mut histogram = [0u32; 256];
    for pixel in image.pixels() {
        let intensity = pixel[0] as usize;
        histogram[intensity] += 1;
    }

    // Build cumulative distribution function (CDF)
    let mut cdf = [0u32; 256];
    cdf[0] = histogram[0];
    for i in 1..256 {
        cdf[i] = cdf[i - 1] + histogram[i];
    }

    // Find minimum CDF value (excluding zeros for proper normalization)
    let cdf_min = cdf.iter().copied().find(|&v| v > 0).unwrap_or(0);

    // Apply equalization: transform each pixel
    let mut output = GrayImage::new(width, height);
    for (y, row) in image.rows().enumerate() {
        let y_coord = y as u32;
        for (x, pixel) in row.enumerate() {
            let x_coord = x as u32;
            let input_value = pixel[0] as u32;
            // Equalization formula: ((cdf - cdf_min) / (total_pixels - cdf_min)) * 255
            let numerator = cdf[input_value as usize] - cdf_min;
            let denominator = total_pixels - cdf_min;
            let new_value = if denominator > 0 {
                ((numerator as f64 / denominator as f64) * 255.0).round() as u8
            } else {
                input_value as u8
            };
            output.put_pixel(x_coord, y_coord, Luma([new_value]));
        }
    }

    output
}

/// Apply linear contrast stretch to enhance image contrast.
///
/// This is an alternative to histogram equalization that stretches
/// the intensity range to use the full 0-255 range.
pub fn apply_contrast_stretch(image: &GrayImage) -> GrayImage {
    let (width, height) = image.dimensions();

    // Find min and max values
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

    // Apply linear stretch
    let mut output = GrayImage::new(width, height);
    let range = (max_val - min_val) as f64;

    for (y, row) in image.rows().enumerate() {
        let y_coord = y as u32;
        for (x, pixel) in row.enumerate() {
            let x_coord = x as u32;
            let val = pixel[0];
            let stretched = ((val as f64 - min_val as f64) / range * 255.0).round() as u8;
            output.put_pixel(x_coord, y_coord, Luma([stretched]));
        }
    }

    output
}

/// Apply binary threshold to a grayscale image.
///
/// Pixels with intensity < 128 become 0 (black).
/// Pixels with intensity >= 128 become 255 (white).
/// Call this after histogram equalization for best results.
fn apply_binarize(image: &GrayImage) -> GrayImage {
    let (width, height) = image.dimensions();
    let mut output = GrayImage::new(width, height);
    for (x, y, pixel) in image.enumerate_pixels() {
        let new_val: u8 = if pixel[0] < 128 { 0 } else { 255 };
        output.put_pixel(x, y, Luma([new_val]));
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_preprocessing_config_defaults() {
        let config = PreprocessingConfig::default();
        assert!(!config.is_enabled(), "Default config should be disabled");

        let optimal = PreprocessingConfig::optimal();
        assert!(optimal.is_enabled(), "Optimal config should be enabled");
    }

    #[test]
    fn test_preprocess_disabled() {
        // Create test image (10x10 white)
        let width = 10u32;
        let height = 10u32;
        let image_data = vec![255u8; (width * height * 4) as usize];

        let config = PreprocessingConfig::default();
        let result = preprocess_image(&image_data, width, height, config);

        assert_eq!(result, image_data, "Should return original when disabled");
    }

    #[test]
    fn test_histogram_equalization() {
        // Create a simple gradient image
        let mut image = GrayImage::new(4, 4);
        for y in 0..4 {
            for x in 0..4 {
                let val = ((x + y) * 255 / 6) as u8;
                image.put_pixel(x, y, Luma([val]));
            }
        }

        let result = apply_histogram_equalization(&image);
        assert_eq!(result.dimensions(), (4, 4));
    }

    #[test]
    fn test_contrast_stretch() {
        // Create image with limited range
        let mut image = GrayImage::new(10, 10);
        for pixel in image.pixels_mut() {
            pixel[0] = 50; // All pixels at value 50
        }

        let result = apply_contrast_stretch(&image);
        // All pixels should become 0 after stretch (min=max=50)
        for pixel in result.pixels() {
            assert_eq!(
                pixel[0], 0,
                "Single-value image should become 0 after stretch"
            );
        }
    }

    #[test]
    fn test_binarize_threshold() {
        // Pixels below 128 → 0 (black), at/above 128 → 255 (white)
        let mut image = GrayImage::new(4, 1);
        image.put_pixel(0, 0, Luma([0]));
        image.put_pixel(1, 0, Luma([127]));
        image.put_pixel(2, 0, Luma([128]));
        image.put_pixel(3, 0, Luma([255]));

        let result = apply_binarize(&image);
        assert_eq!(result.get_pixel(0, 0)[0], 0, "0 → black");
        assert_eq!(result.get_pixel(1, 0)[0], 0, "127 → black");
        assert_eq!(result.get_pixel(2, 0)[0], 255, "128 → white");
        assert_eq!(result.get_pixel(3, 0)[0], 255, "255 → white");
    }

    #[test]
    fn test_full_pipeline_binarized_output() {
        // Run full grayscale → EQ → binarize pipeline; output must be only 0 or 255
        let width = 10u32;
        let height = 10u32;
        let mut image_data = Vec::with_capacity((width * height * 4) as usize);
        for i in 0..(width * height) {
            let val = ((i * 255) / (width * height)) as u8;
            image_data.extend_from_slice(&[val, val, val, 255u8]); // BGRA
        }

        let config = PreprocessingConfig {
            grayscale: true,
            contrast_enhancement: true,
            binarize: true,
        };

        let result = preprocess_image(&image_data, width, height, config);

        assert_eq!(result.len(), (width * height * 4) as usize, "output size");
        for chunk in result.chunks(4) {
            let b = chunk[0];
            assert!(b == 0 || b == 255, "expected 0 or 255, got {}", b);
            assert_eq!(chunk[0], chunk[1], "B == G");
            assert_eq!(chunk[1], chunk[2], "G == R");
            assert_eq!(chunk[3], 255, "alpha == 255");
        }
    }
}
