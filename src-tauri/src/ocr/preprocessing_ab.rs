// =============================================================================
// PREPROCESSING_AB.RS - Which preprocessing variant reads a real frame best
// =============================================================================
// Issue #53 asks whether hard binarisation is what splits Chinese glyphs into
// their radicals. Thresholding to pure black and white erodes thin connecting
// strokes, which is exactly that failure mode, and `recognize_multi_pass`
// already declines to binarise on every one of its passes - but nobody had put
// the variants side by side on the same frame and read the results.
//
// This is that comparison. It is not a unit test and asserts nothing: Windows
// OCR is the thing under measurement, so the output has to be judged by a
// person against what the frame actually says.
//
// Synthetic text will not reproduce the defect. Capture a real subtitle frame,
// then:
//
// ```powershell
// $env:MEOWCAL_OCR_AB_FRAME = "D:\tmp\subtitle-frame.png"
// cargo test --lib preprocessing_variants -- --ignored --nocapture
// ```
//
// The variant that splits fewest characters wins; that is the evidence #53
// asks for before the shipped default is changed.
// =============================================================================

use super::{PreprocessingConfig, WindowsOcr};

/// The variants, weakest preprocessing last. `None` means the frame reaches
/// Windows OCR exactly as captured.
const VARIANTS: [(&str, Option<PreprocessingConfig>); 4] = [
    (
        "shipped default (grayscale + contrast + binarize)",
        Some(PreprocessingConfig {
            grayscale: true,
            contrast_enhancement: true,
            binarize: true,
        }),
    ),
    (
        "no binarize (grayscale + contrast)",
        Some(PreprocessingConfig {
            grayscale: true,
            contrast_enhancement: true,
            binarize: false,
        }),
    ),
    (
        "grayscale only",
        Some(PreprocessingConfig {
            grayscale: true,
            contrast_enhancement: false,
            binarize: false,
        }),
    ),
    ("no preprocessing", None),
];

#[tokio::test]
#[ignore = "needs a captured subtitle frame in MEOWCAL_OCR_AB_FRAME"]
async fn preprocessing_variants_over_a_captured_frame() {
    let path = std::env::var("MEOWCAL_OCR_AB_FRAME")
        .expect("set MEOWCAL_OCR_AB_FRAME to a captured subtitle frame");
    let frame = image::open(&path)
        .unwrap_or_else(|e| panic!("failed to read {path}: {e}"))
        .to_rgba8();
    let (width, height) = frame.dimensions();

    // The capture path hands OCR BGRA; `image` decodes RGBA.
    let bgra: Vec<u8> = frame
        .pixels()
        .flat_map(|p| [p[2], p[1], p[0], p[3]])
        .collect();

    let ocr = WindowsOcr::new().expect("OCR engine");
    println!("frame {path} ({width}x{height})\n");

    for (label, config) in VARIANTS {
        let result = match config {
            Some(config) => {
                ocr.recognize_with_preprocessing(&bgra, width, height, config)
                    .await
            }
            None => {
                ocr.recognize_without_preprocessing(&bgra, width, height)
                    .await
            }
        };
        match result {
            Ok(result) => println!(
                "{label}\n  {} significant chars in {} line(s)\n  {}\n",
                result.significant_chars(),
                result.lines.len(),
                result.text
            ),
            Err(error) => println!("{label}\n  FAILED: {error}\n"),
        }
    }
}
