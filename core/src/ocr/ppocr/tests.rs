use super::*;
use std::sync::OnceLock;

/// Loading the models takes a moment; share one engine across tests.
fn engine() -> &'static PpOcr {
    static ENGINE: OnceLock<PpOcr> = OnceLock::new();
    ENGINE.get_or_init(|| PpOcr::new().unwrap())
}

/// Decode an RGB(A) PNG fixture into BGRA.
fn fixture(png_bytes: &[u8]) -> (Vec<u8>, u32, u32) {
    let mut reader = png::Decoder::new(std::io::Cursor::new(png_bytes))
        .read_info()
        .unwrap();
    let mut buffer = vec![0; reader.output_buffer_size().unwrap()];
    let info = reader.next_frame(&mut buffer).unwrap();
    let channels = info.color_type.samples();
    let bgra = buffer[..info.buffer_size()]
        .chunks_exact(channels)
        .flat_map(|pixel| [pixel[2], pixel[1], pixel[0], 255])
        .collect();
    (bgra, info.width, info.height)
}

fn read(png_bytes: &[u8]) -> NativeResult {
    let (bgra, width, height) = fixture(png_bytes);
    engine().recognize_bgra(&bgra, width, height).unwrap()
}

#[test]
fn embedded_models_are_the_reviewed_files() {
    let digest = |bytes: &[u8]| {
        crate::sha256::encode_hex(&crate::sha256::digest_reader(&mut &bytes[..]).unwrap())
    };
    assert_eq!(
        digest(DETECTOR),
        "090f04abcd9d9a7498bc4ebf677e4cb9bdce1fe4197ddb7e529f1ef44e1ff94f"
    );
    assert_eq!(
        digest(RECOGNIZER),
        "6f327246b50388f3c176ae304bd95767ea6dc0c9ae92153ef8cbe210b3c14884"
    );
    // Blank, dictionary, space: the recognizer's class count.
    assert_eq!(engine().charset.len() + 1, 18710);
}

#[test]
fn reads_a_chinese_subtitle_line() {
    let result = read(include_bytes!(
        "../../../tests/fixtures/ocr/subtitle-zh.png"
    ));
    assert_eq!(result.lines, ["谢谢你一直陪在我身边"]);
    assert_eq!(result.text, "谢谢你一直陪在我身边");
    assert_eq!(result.frame_width, 720.0);
}

#[test]
fn reads_an_english_subtitle_line_inside_the_frame() {
    let result = read(include_bytes!(
        "../../../tests/fixtures/ocr/subtitle-en.png"
    ));
    assert_eq!(result.lines, ["Don't worry, everything will be fine."]);
    let line = result.boxes[0];
    assert!(line.x > 40.0 && line.x + line.width < 680.0, "{line:?}");
    assert!(line.y >= 0.0 && line.y + line.height <= 72.0, "{line:?}");
    assert!(line.height > 20.0, "{line:?}");
}

#[test]
fn a_blank_frame_reads_nothing() {
    let result = engine()
        .recognize_bgra(&vec![40; 200 * 60 * 4], 200, 60)
        .unwrap();
    assert!(result.lines.is_empty() && result.boxes.is_empty());
}

#[test]
fn frames_must_match_their_dimensions() {
    assert!(matches!(
        engine().recognize_bgra(&[0; 12], 2, 2),
        Err(OcrError::Frame(_))
    ));
}

#[test]
fn oversized_and_tiny_images_are_brought_into_bounds() {
    let image = |width, height| Bgr {
        width,
        height,
        data: vec![0; width * height * 3],
    };
    let large = within_bounds(&image(4000, 500));
    // Python rounds 2000 / 32 = 62.5 to 62.
    assert_eq!((large.width, large.height), (1984, 256));
    let small = within_bounds(&image(300, 10));
    assert_eq!((small.width, small.height), (896, 32));
}
