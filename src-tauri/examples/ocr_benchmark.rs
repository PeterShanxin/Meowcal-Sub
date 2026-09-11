//! Opt-in synthetic-frame OCR benchmark. Capture, translation and UI are excluded.
use meowcal_sub::ocr::WindowsOcr;
use serde_json::json;
use std::path::PathBuf;
use std::time::Instant;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() != 8 {
        return Err(
            "Usage: ocr_benchmark FRAME WIDTH HEIGHT LANGUAGE POLICY RUNS WARMUP OUTPUT".into(),
        );
    }
    let pixels = std::fs::read(&args[0])?;
    let width = args[1].parse()?;
    let height = args[2].parse()?;
    let runs: usize = args[5].parse()?;
    let warmup: usize = args[6].parse()?;
    if runs == 0 || warmup == 0 || !matches!(args[4].as_str(), "single" | "raw" | "multi") {
        return Err("Positive runs/warmup and single, raw or multi policy required".into());
    }
    let output = PathBuf::from(&args[7]);
    meowcal_sub::core_client::register_headless(Some(output.with_extension("storage")), vec![])?;
    let _shutdown = scopeguard::guard((), |_| meowcal_sub::core_client::shutdown_owned());
    let initialized = Instant::now();
    let ocr = WindowsOcr::with_language(&args[3])?;
    let initialization_ms = initialized.elapsed().as_secs_f64() * 1000.0;
    let mut samples = Vec::with_capacity(runs);
    let mut outputs = Vec::with_capacity(runs);
    let mut warmup_outputs = Vec::with_capacity(warmup);
    let mut warmup_samples = Vec::with_capacity(warmup);
    let mut first_ms = 0.0;
    for index in 0..runs + warmup {
        let started = Instant::now();
        let result = match args[4].as_str() {
            "single" => ocr.recognize(&pixels, width, height).await?,
            "raw" => {
                ocr.recognize_without_preprocessing(&pixels, width, height)
                    .await?
            }
            _ => ocr.recognize_multi_pass(&pixels, width, height, 3).await?,
        };
        let elapsed = started.elapsed().as_secs_f64() * 1000.0;
        if index == 0 {
            first_ms = elapsed;
        }
        let boxes: Vec<_> = result
            .boxes
            .iter()
            .map(|b| json!({"x":b.x,"y":b.y,"width":b.width,"height":b.height}))
            .collect();
        let result = json!({"text":result.text,"lines":result.lines,
            "boxes":boxes,"frameWidth":result.frame_width});
        if index >= warmup {
            samples.push(elapsed);
            outputs.push(result);
        } else {
            warmup_samples.push(elapsed);
            warmup_outputs.push(result);
        }
    }
    let report = json!({"width":width,"height":height,"policy":args[4],
        "initializationMs":initialization_ms,"firstCallMs":first_ms,
        "samplesMs":samples,"outputs":outputs,
        "warmupSamplesMs":warmup_samples,"warmupOutputs":warmup_outputs});
    std::fs::write(output, serde_json::to_vec_pretty(&report)?)?;
    Ok(())
}
