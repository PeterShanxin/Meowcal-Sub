use super::*;

/// Build a subtitle-shaped frame: a video-like background gradient with a few
/// bright glyph columns on top. `bg_range` is inclusive.
///
/// Returns the BGRA bytes plus a per-pixel mask marking the glyph pixels.
fn subtitle_frame(
    width: u32,
    height: u32,
    bg_range: (u8, u8),
    text_value: u8,
) -> (Vec<u8>, Vec<bool>) {
    let mut data = Vec::with_capacity((width * height * 4) as usize);
    let mut is_text = Vec::with_capacity((width * height) as usize);
    let span = (bg_range.1 - bg_range.0) as u32;
    for y in 0..height {
        for x in 0..width {
            // Glyph columns cover roughly 8% of the strip, like real text.
            let text_pixel = x % 12 == 0 && (2..height - 2).contains(&y);
            let value = if text_pixel {
                text_value
            } else {
                (bg_range.0 as u32 + (x * span) / width.max(1)) as u8
            };
            data.extend_from_slice(&[value, value, value, 255]);
            is_text.push(text_pixel);
        }
    }
    (data, is_text)
}

fn shade_at(result: &[u8], index: usize) -> u8 {
    result[index * 4]
}

/// Share of background pixels that ended up the same shade as the glyphs.
fn background_bleed(result: &[u8], is_text: &[bool]) -> f32 {
    let glyph_index = is_text.iter().position(|text| *text).expect("glyph pixels");
    let glyph_shade = shade_at(result, glyph_index);
    let background: Vec<usize> = is_text
        .iter()
        .enumerate()
        .filter(|(_, text)| !**text)
        .map(|(index, _)| index)
        .collect();
    let bled = background
        .iter()
        .filter(|index| shade_at(result, **index) == glyph_shade)
        .count();
    bled as f32 / background.len() as f32
}

#[test]
fn test_preprocessing_config_defaults() {
    let config = PreprocessingConfig::default();
    assert!(!config.is_enabled(), "Default config should be disabled");

    let optimal = PreprocessingConfig::optimal();
    assert!(optimal.is_enabled(), "Optimal config should be enabled");
}

#[test]
fn test_preprocess_disabled() {
    let image_data = vec![255u8; 10 * 10 * 4];
    let result = preprocess_image(&image_data, 10, 10, PreprocessingConfig::default());
    assert_eq!(result, image_data, "Should return original when disabled");
}

// A subtitle strip is overwhelmingly background, so a threshold derived from the
// pixel population - histogram equalisation feeding a fixed 128 cut - promoted
// the brighter half of the background to the same white as the glyphs. OCR then
// read blocks instead of letters, which is what made plainly legible subtitles
// come back unreadable. The equalising build failed this at 46.3%.
#[test]
fn separates_bright_text_from_a_video_background() {
    let (data, is_text) = subtitle_frame(120, 24, (10, 90), 240);

    let result = preprocess_image(&data, 120, 24, PreprocessingConfig::optimal());

    let bled = background_bleed(&result, &is_text);
    assert!(
        bled < 0.05,
        "{:.1}% of the background was flattened into the glyph shade",
        bled * 100.0
    );
}

// Windows OCR is most reliable on dark text over a light page, and subtitles
// arrive the other way round.
#[test]
fn normalizes_light_text_on_dark_video_to_dark_on_light() {
    let (data, is_text) = subtitle_frame(120, 24, (10, 90), 240);

    let result = preprocess_image(&data, 120, 24, PreprocessingConfig::optimal());

    let glyph = is_text.iter().position(|text| *text).unwrap();
    let background = is_text.iter().position(|text| !*text).unwrap();
    assert_eq!(shade_at(&result, glyph), 0, "glyphs should end up black");
    assert_eq!(
        shade_at(&result, background),
        255,
        "background should end up white"
    );
}

// Burned-in subtitles are sometimes dark text on a bright scene. The minority
// cluster is still the text, so polarity must flip with the image.
#[test]
fn normalizes_dark_text_on_a_bright_background_the_same_way() {
    let (data, is_text) = subtitle_frame(120, 24, (170, 250), 15);

    let result = preprocess_image(&data, 120, 24, PreprocessingConfig::optimal());

    let glyph = is_text.iter().position(|text| *text).unwrap();
    let background = is_text.iter().position(|text| !*text).unwrap();
    assert_eq!(shade_at(&result, glyph), 0, "glyphs should end up black");
    assert_eq!(
        shade_at(&result, background),
        255,
        "background should end up white"
    );
    assert!(background_bleed(&result, &is_text) < 0.05);
}

#[test]
fn otsu_splits_two_clusters_between_them() {
    let mut histogram = [0u32; 256];
    histogram[20] = 950; // background
    histogram[200] = 50; // glyphs
    let threshold = otsu_threshold(&histogram);
    assert!(
        (20..200).contains(&(threshold as u32)),
        "threshold {} should fall between the clusters",
        threshold
    );
}

#[test]
fn otsu_survives_a_blank_region() {
    // A frame with nothing in it must not panic or divide by zero.
    let histogram = [0u32; 256];
    assert_eq!(otsu_threshold(&histogram), 127);
}

#[test]
fn test_contrast_stretch() {
    let mut image = GrayImage::new(10, 10);
    for pixel in image.pixels_mut() {
        pixel[0] = 50;
    }

    let result = apply_contrast_stretch(&image);
    for pixel in result.pixels() {
        assert_eq!(
            pixel[0], 0,
            "Single-value image should become 0 after stretch"
        );
    }
}

#[test]
fn test_full_pipeline_binarized_output() {
    let width = 10u32;
    let height = 10u32;
    let mut image_data = Vec::with_capacity((width * height * 4) as usize);
    for i in 0..(width * height) {
        let val = ((i * 255) / (width * height)) as u8;
        image_data.extend_from_slice(&[val, val, val, 255u8]); // BGRA
    }

    let result = preprocess_image(&image_data, width, height, PreprocessingConfig::optimal());

    assert_eq!(result.len(), (width * height * 4) as usize, "output size");
    for chunk in result.chunks(4) {
        let b = chunk[0];
        assert!(b == 0 || b == 255, "expected 0 or 255, got {}", b);
        assert_eq!(chunk[0], chunk[1], "B == G");
        assert_eq!(chunk[1], chunk[2], "G == R");
        assert_eq!(chunk[3], 255, "alpha == 255");
    }
}
