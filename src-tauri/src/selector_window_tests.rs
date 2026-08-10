use super::*;

use base64::Engine;

fn decode_png(data_url: &str) -> (Vec<u8>, u32, u32) {
    let base64_payload = data_url
        .strip_prefix("data:image/png;base64,")
        .expect("data URL prefix");
    let png_bytes = base64::engine::general_purpose::STANDARD
        .decode(base64_payload)
        .expect("base64 payload");

    let decoder = png::Decoder::new(png_bytes.as_slice());
    let mut reader = decoder.read_info().expect("png header");
    let mut pixels = vec![0u8; reader.output_buffer_size()];
    let info = reader.next_frame(&mut pixels).expect("png frame");
    pixels.truncate(info.buffer_size());

    (pixels, info.width, info.height)
}

/// The capture backends hand back BGRA to match the Windows APIs; a PNG that
/// keeps that order renders with red and blue swapped.
#[test]
fn bgra_capture_bytes_are_written_as_rgba() {
    // One opaque red pixel and one opaque blue pixel, in BGRA.
    let capture = CaptureResult::new(vec![0, 0, 255, 255, 255, 0, 0, 255], 2, 1);

    let snapshot = encode_snapshot(capture, 2, 1).expect("encode");
    let (pixels, width, height) = decode_png(&snapshot.data_url);

    assert_eq!((width, height), (2, 1));
    assert_eq!(pixels, vec![255, 0, 0, 255, 0, 0, 255, 255]);
}

#[test]
fn the_snapshot_is_an_inline_png_data_url() {
    let capture = CaptureResult::new(vec![1, 2, 3, 4], 1, 1);

    let snapshot = encode_snapshot(capture, 1, 1).expect("encode");

    assert!(snapshot.data_url.starts_with("data:image/png;base64,"));
    assert!(snapshot.data_url.len() > "data:image/png;base64,".len());
}

/// The reported size is the screen the selector will cover, which is what the
/// webview maps its selection rectangle against — not whatever the capture
/// backend happened to return.
#[test]
fn the_reported_size_is_the_requested_screen_size() {
    let capture = CaptureResult::new(vec![0; 16], 2, 2);

    let snapshot = encode_snapshot(capture, 1920, 1080).expect("encode");

    assert_eq!((snapshot.width, snapshot.height), (1920, 1080));
}

#[test]
fn a_frame_smaller_than_its_declared_size_is_refused_rather_than_encoded() {
    // Two pixels of data described as a 2x2 frame.
    let capture = CaptureResult::new(vec![0; 8], 2, 2);

    let error = match encode_snapshot(capture, 2, 2) {
        Err(error) => error,
        Ok(_) => panic!("a short frame should not encode"),
    };

    assert!(
        error.starts_with("PNG encoding failed:"),
        "unexpected error: {error}"
    );
}

/// The UI branches on this discriminant to tell the user which selector it
/// got, so the wire spelling is part of the command contract.
#[test]
fn the_selector_mode_serializes_as_the_ui_expects() {
    let winui = serde_json::to_value(OpenAreaSelectorResult {
        mode: AreaSelectorMode::Winui,
    })
    .expect("serialize winui");
    let legacy = serde_json::to_value(OpenAreaSelectorResult {
        mode: AreaSelectorMode::Legacy,
    })
    .expect("serialize legacy");

    assert_eq!(winui, serde_json::json!({ "mode": "winui" }));
    assert_eq!(legacy, serde_json::json!({ "mode": "legacy" }));
}

#[test]
fn the_snapshot_payload_is_camel_case() {
    let value = serde_json::to_value(SelectorSnapshot {
        data_url: "data:image/png;base64,AA==".to_string(),
        width: 3,
        height: 4,
    })
    .expect("serialize snapshot");

    assert_eq!(
        value,
        serde_json::json!({ "dataUrl": "data:image/png;base64,AA==", "width": 3, "height": 4 })
    );
}
