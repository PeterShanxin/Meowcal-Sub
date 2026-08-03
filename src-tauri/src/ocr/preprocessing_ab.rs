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
// $env:MEOWCAL_OCR_AB_LANG  = "zh-CN"   # optional, this is the default
// cargo test --lib preprocessing_variants -- --ignored --nocapture
// ```
//
// The language matters: `WindowsOcr::new` builds an engine from the user's
// profile languages, which on an English install reads a Chinese subtitle as
// nothing at all. The pipeline passes the configured source language, so this
// does too.
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
    let target = std::env::var("MEOWCAL_OCR_AB_FRAME")
        .expect("set MEOWCAL_OCR_AB_FRAME to a captured frame or a directory of them");
    let language = std::env::var("MEOWCAL_OCR_AB_LANG").unwrap_or_else(|_| "zh-CN".to_string());
    let ocr = WindowsOcr::with_language(&language)
        .unwrap_or_else(|e| panic!("OCR engine for {language}: {e}"));

    let mut frames: Vec<std::path::PathBuf> = if std::path::Path::new(&target).is_dir() {
        std::fs::read_dir(&target)
            .expect("read frame directory")
            .filter_map(|entry| entry.ok().map(|e| e.path()))
            .filter(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("png")))
            .collect()
    } else {
        vec![std::path::PathBuf::from(&target)]
    };
    frames.sort();
    println!("{} frame(s), engine {language}\n", frames.len());

    // Rows where the variants disagree are the whole point: those are the
    // frames where preprocessing decided the result.
    let verbose = std::env::var_os("MEOWCAL_OCR_AB_VERBOSE").is_some();
    let scale: Option<f64> = std::env::var("MEOWCAL_OCR_AB_SCALE")
        .ok()
        .and_then(|s| s.parse().ok());
    // With MEOWCAL_OCR_AB_TRUTH set to what the frames actually say, each
    // variant is scored against it instead of only against the others. That is
    // the character-level accuracy #53 asks for, and it needs a human to have
    // read the frame first.
    let truth = std::env::var("MEOWCAL_OCR_AB_TRUTH")
        .ok()
        .map(|t| crate::ocr_stability::normalize(&t));
    let mut accuracy = [0f32; VARIANTS.len()];
    let mut scored = 0usize;
    let mut disagreements = 0usize;
    let mut with_text = 0usize;
    let mut blank = 0usize;
    for frame_path in &frames {
        let frame = match image::open(frame_path) {
            Ok(frame) => frame.to_rgba8(),
            Err(error) => {
                println!("{}: unreadable: {error}", frame_path.display());
                continue;
            }
        };
        let (width, height) = frame.dimensions();
        // The capture path hands OCR BGRA; `image` decodes RGBA.
        let bgra: Vec<u8> = frame
            .pixels()
            .flat_map(|p| [p[2], p[1], p[0], p[3]])
            .collect();

        // MEOWCAL_OCR_AB_SCALE reduces every frame first, which is how the
        // question "how small can a frame get before the read suffers" is
        // answered against real content rather than guessed at.
        let (bgra, width, height) = match scale {
            Some(scale) if scale < 1.0 => {
                let target_width = (((f64::from(width) * scale).round()) as u32).max(1);
                let target_height = (((f64::from(height) * scale).round()) as u32).max(1);
                let (target_width, target_height): (u32, u32) = (target_width, target_height);
                match crate::ocr::frame_budget::scale_bgra(
                    &bgra,
                    width,
                    height,
                    target_width,
                    target_height,
                ) {
                    Some(scaled) => (scaled, target_width, target_height),
                    None => (bgra, width, height),
                }
            }
            _ => (bgra, width, height),
        };

        let mut reads: Vec<(&str, String)> = Vec::new();
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
            reads.push((
                label,
                result.map_or_else(|e| format!("<failed: {e}>"), |r| r.text),
            ));
        }

        // A frame nobody read carries no evidence either way. Counting those
        // separately is the difference between "preprocessing does not matter"
        // and "there was nothing on screen".
        if reads.iter().all(|(_, text)| text.trim().is_empty()) {
            blank += 1;
            continue;
        }
        with_text += 1;

        if let Some(truth) = &truth {
            scored += 1;
            for (index, (_, text)) in reads.iter().enumerate() {
                let read = crate::ocr_stability::normalize(text);
                accuracy[index] += crate::ocr_stability::similarity(truth, &read);
            }
        }

        let agreed = reads.iter().all(|(_, text)| *text == reads[0].1);
        if agreed {
            // Agreement is only reassuring if the agreed read is right, so
            // MEOWCAL_OCR_AB_VERBOSE shows it for eyeballing against the frame.
            if verbose {
                println!(
                    "{} (all agree)  {}",
                    frame_path.file_name().unwrap_or_default().display(),
                    reads[0].1
                );
            }
            continue;
        }
        disagreements += 1;
        println!("{}", frame_path.file_name().unwrap_or_default().display());
        for (label, text) in &reads {
            println!("  {:<50} {text}", format!("{label}:"));
        }
        println!();
    }

    println!(
        "{} frame(s): {with_text} carried text, {blank} read empty by every variant.\n\
         {disagreements} of the {with_text} read differently depending on preprocessing.",
        frames.len()
    );

    if scored > 0 {
        println!("\nmean character accuracy against the supplied truth, over {scored} frame(s):");
        let mut ranked: Vec<(f32, &str)> = accuracy
            .iter()
            .zip(VARIANTS.iter().map(|(label, _)| *label))
            .map(|(total, label)| (total / scored as f32, label))
            .collect();
        ranked.sort_by(|a, b| b.0.total_cmp(&a.0));
        for (mean, label) in ranked {
            println!("  {:>6.1}%  {label}", mean * 100.0);
        }
    }
}
