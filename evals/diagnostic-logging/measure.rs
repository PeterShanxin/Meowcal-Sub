use std::{
    io::{self, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::Instant,
};

const MAX_FRAME_BYTES: usize = 64 * 1024 * 1024;
const RECORD_LIMIT: u64 = 256 * 1024 * 1024;
const WIDTH: u32 = 1200;
const HEIGHT: u32 = 120;
static NEXT: AtomicU64 = AtomicU64::new(0);

fn synthetic_frames() -> (Vec<u8>, Vec<u8>) {
    let mut raw = Vec::new();
    let mut processed = Vec::new();
    let mut seed = 12345u32;
    for y in 0..HEIGHT {
        for x in 0..WIDTH {
            seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
            let mark = (64..100).contains(&y)
                && (171..1000).contains(&x)
                && x % 21 >= 5
                && (y % 13 <= 3 || x % 21 <= 8);
            let shade = if mark {
                240
            } else {
                ((seed >> 28) as u8).saturating_add((x / 24 + y / 12) as u8)
            };
            raw.extend([shade, shade.saturating_add(4), shade.saturating_add(7), 255]);
            let binarized = if mark { 255 } else { 0 };
            processed.extend([binarized, binarized, binarized, 255]);
        }
    }
    (raw, processed)
}

fn measure(root: &Path, runs: usize, retained: usize) -> io::Result<()> {
    std::fs::create_dir_all(root)?;
    for index in 0..retained {
        let dir = root.join(format!("record-{index:020}-1-0"));
        std::fs::create_dir(&dir)?;
        std::fs::write(dir.join("record.json"), b"{}")?;
        std::fs::write(dir.join("input.png"), [0; 8192])?;
    }
    let (raw, processed) = synthetic_frames();
    let (mut total, mut encode, mut store) = (Vec::new(), Vec::new(), Vec::new());
    for _ in 0..runs {
        let start = Instant::now();
        let (mut encode_ms, mut store_ms) = (0.0, 0.0);
        for frame in [&raw, &processed] {
            let timer = Instant::now();
            let png = encode_frame(frame.clone(), WIDTH, HEIGHT)?;
            encode_ms += timer.elapsed().as_secs_f64() * 1000.0;
            let timer = Instant::now();
            write_record(root, b"{}", Some(&png), RECORD_LIMIT, || true)?;
            store_ms += timer.elapsed().as_secs_f64() * 1000.0;
        }
        total.push(start.elapsed().as_secs_f64() * 1000.0);
        encode.push(encode_ms);
        store.push(store_ms);
    }
    println!("retained={retained} samples={runs} (ms per pair):");
    for (label, mut values) in [("total", total), ("encode", encode), ("store", store)] {
        values.sort_by(f64::total_cmp);
        println!(
            "  {label}: p50={:.2} p95={:.2} max={:.2}",
            values[runs.div_ceil(2) - 1],
            values[(95 * runs).div_ceil(100) - 1],
            values[runs - 1]
        );
    }
    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args_os().skip(1);
    let root = args
        .next()
        .map(PathBuf::from)
        .ok_or("Missing evidence directory")?;
    let runs = args
        .next()
        .ok_or("Missing sample count")?
        .to_string_lossy()
        .parse::<usize>()?;
    let retained = args
        .next()
        .ok_or("Missing retained count")?
        .to_string_lossy()
        .parse::<usize>()?;
    if !(3..=200).contains(&runs) || !(1..=2000).contains(&retained) {
        return Err("Sample count or retained count is out of range".into());
    }
    measure(&root.join("fresh"), runs, 0)?;
    measure(&root.join("retained"), runs, retained)?;
    Ok(())
}
