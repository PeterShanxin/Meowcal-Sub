//! Area-selector snapshot encoding benchmark.
//!
//! `baseline` is a verbatim copy of `selector_window::encode_snapshot` at the
//! benchmark's base commit. The other variants change only the PNG colour type
//! and compression preset (or switch to JPEG) so each change can be attributed.
//! Every variant runs through the same timing loop and the same serde_json
//! serialisation Tauri applies to an emitted event payload and a command result.
//!
//! Usage: snapshot-bench <fixtures-dir> <runs> <warmup> <results.json>

use base64::Engine;
use serde::Serialize;
use std::time::Instant;

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct SelectorSnapshot {
    data_url: String,
    width: i32,
    height: i32,
}

#[derive(Clone, Copy)]
enum Variant {
    Baseline,
    Png { rgb: bool, compression: png::Compression },
    Jpeg { quality: u8 },
}

impl Variant {
    fn name(self) -> String {
        match self {
            Variant::Baseline => "baseline-rgba-balanced".into(),
            Variant::Png { rgb, compression } => format!(
                "{}-{:?}",
                if rgb { "rgb" } else { "rgba" },
                compression
            )
            .to_lowercase(),
            Variant::Jpeg { quality } => format!("jpeg-q{quality}"),
        }
    }
}

/// Verbatim body of `encode_snapshot` at the base commit.
fn encode_baseline(data: Vec<u8>, w: u32, h: u32) -> String {
    let mut rgba = data;
    for px in rgba.as_chunks_mut::<4>().0 {
        px.swap(0, 2);
    }
    let mut png_bytes = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut png_bytes, w, h);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().unwrap();
        writer.write_image_data(&rgba).unwrap();
    }
    let b64 = base64::engine::general_purpose::STANDARD.encode(&png_bytes);
    format!("data:image/png;base64,{}", b64)
}

fn bgra_to_rgb(data: &[u8]) -> Vec<u8> {
    let mut rgb = Vec::with_capacity(data.len() / 4 * 3);
    for px in data.as_chunks::<4>().0 {
        rgb.extend_from_slice(&[px[2], px[1], px[0]]);
    }
    rgb
}

fn encode_png(data: Vec<u8>, w: u32, h: u32, rgb: bool, compression: png::Compression) -> String {
    let (pixels, color) = if rgb {
        (bgra_to_rgb(&data), png::ColorType::Rgb)
    } else {
        let mut rgba = data;
        for px in rgba.as_chunks_mut::<4>().0 {
            px.swap(0, 2);
        }
        (rgba, png::ColorType::Rgba)
    };
    let mut png_bytes = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut png_bytes, w, h);
        encoder.set_color(color);
        encoder.set_depth(png::BitDepth::Eight);
        encoder.set_compression(compression);
        let mut writer = encoder.write_header().unwrap();
        writer.write_image_data(&pixels).unwrap();
    }
    let b64 = base64::engine::general_purpose::STANDARD.encode(&png_bytes);
    format!("data:image/png;base64,{}", b64)
}

fn encode_jpeg(data: Vec<u8>, w: u32, h: u32, quality: u8) -> String {
    let rgb = bgra_to_rgb(&data);
    let mut out = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, quality)
        .encode(&rgb, w, h, image::ExtendedColorType::Rgb8)
        .unwrap();
    let b64 = base64::engine::general_purpose::STANDARD.encode(&out);
    format!("data:image/jpeg;base64,{}", b64)
}

fn encode(variant: Variant, data: Vec<u8>, w: u32, h: u32) -> String {
    match variant {
        Variant::Baseline => encode_baseline(data, w, h),
        Variant::Png { rgb, compression } => encode_png(data, w, h, rgb, compression),
        Variant::Jpeg { quality } => encode_jpeg(data, w, h, quality),
    }
}

/// Decode a PNG data URL back to RGB and compare with the source frame.
fn png_is_lossless(data_url: &str, source_bgra: &[u8]) -> bool {
    let payload = data_url.split_once(',').unwrap().1;
    let bytes = base64::engine::general_purpose::STANDARD.decode(payload).unwrap();
    let mut reader = png::Decoder::new(std::io::Cursor::new(bytes)).read_info().unwrap();
    let mut buf = vec![0u8; reader.output_buffer_size().unwrap()];
    let info = reader.next_frame(&mut buf).unwrap();
    let channels = info.color_type.samples();
    buf.truncate(info.buffer_size());
    buf.chunks_exact(channels)
        .zip(source_bgra.as_chunks::<4>().0)
        .all(|(out, src)| out[0] == src[2] && out[1] == src[1] && out[2] == src[0])
}

fn stats(mut xs: Vec<f64>) -> serde_json::Value {
    xs.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let pct = |p: f64| xs[((xs.len() as f64 - 1.0) * p).round() as usize];
    serde_json::json!({
        "n": xs.len(), "min": xs[0], "p50": pct(0.5), "p95": pct(0.95), "max": xs[xs.len() - 1],
        "mean": xs.iter().sum::<f64>() / xs.len() as f64, "samples": xs,
    })
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let dir = std::path::PathBuf::from(&args[1]);
    let runs: usize = args[2].parse().unwrap();
    let warmup: usize = args[3].parse().unwrap();
    let out_path = &args[4];
    let manifest: serde_json::Value =
        serde_json::from_slice(&std::fs::read(dir.join("fixtures.json")).unwrap()).unwrap();
    let variants = [
        Variant::Baseline,
        Variant::Png { rgb: false, compression: png::Compression::Fast },
        Variant::Png { rgb: false, compression: png::Compression::Fastest },
        Variant::Png { rgb: true, compression: png::Compression::Balanced },
        Variant::Png { rgb: true, compression: png::Compression::Fast },
        Variant::Png { rgb: true, compression: png::Compression::Fastest },
        Variant::Jpeg { quality: 85 },
    ];
    let only: Option<Vec<String>> = std::env::var("VARIANTS")
        .ok()
        .map(|v| v.split(',').map(str::to_string).collect());
    std::fs::create_dir_all(dir.join("out")).unwrap();

    let mut results = Vec::new();
    for fixture in manifest.as_array().unwrap() {
        let name = fixture["name"].as_str().unwrap();
        let w = fixture["width"].as_u64().unwrap() as u32;
        let h = fixture["height"].as_u64().unwrap() as u32;
        let source = std::fs::read(dir.join(fixture["file"].as_str().unwrap())).unwrap();
        for variant in variants {
            if let Some(only) = &only {
                if !only.contains(&variant.name()) {
                    continue;
                }
            }
            let mut encode_ms = Vec::new();
            let mut json_ms = Vec::new();
            let mut last = String::new();
            for i in 0..warmup + runs {
                // The copy mirrors the capture buffer the real function takes by value.
                let frame = source.clone();
                let t0 = Instant::now();
                let data_url = encode(variant, frame, w, h);
                let t1 = Instant::now();
                let snapshot = SelectorSnapshot { data_url, width: w as i32, height: h as i32 };
                let json = serde_json::to_string(&snapshot).unwrap();
                let t2 = Instant::now();
                std::hint::black_box(&json);
                if i >= warmup {
                    encode_ms.push((t1 - t0).as_secs_f64() * 1e3);
                    json_ms.push((t2 - t1).as_secs_f64() * 1e3);
                }
                last = snapshot.data_url;
            }
            let lossless = match variant {
                Variant::Jpeg { .. } => false,
                _ => png_is_lossless(&last, &source),
            };
            let out_file = dir.join("out").join(format!("{name}.{}.json", variant.name()));
            std::fs::write(
                &out_file,
                serde_json::to_string(&SelectorSnapshot { data_url: last.clone(), width: w as i32, height: h as i32 }).unwrap(),
            )
            .unwrap();
            let e = stats(encode_ms);
            eprintln!(
                "{name:24} {:24} encode p50 {:8.1} ms  p95 {:8.1} ms  data_url {:6.2} MB  lossless {lossless}",
                variant.name(),
                e["p50"].as_f64().unwrap(),
                e["p95"].as_f64().unwrap(),
                last.len() as f64 / 1e6
            );
            results.push(serde_json::json!({
                "fixture": name, "width": w, "height": h, "variant": variant.name(),
                "dataUrlBytes": last.len(), "lossless": lossless,
                "encodeMs": e, "jsonSerializeMs": stats(json_ms),
            }));
        }
    }
    std::fs::write(out_path, serde_json::to_string_pretty(&results).unwrap()).unwrap();
}
