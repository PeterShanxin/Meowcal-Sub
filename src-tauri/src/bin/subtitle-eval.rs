use meowcal_sub::config::AppConfig;
use meowcal_sub::llm::{FoundryLocalBackend, TranslatorBackend};
use meowcal_sub::subtitle_eval::{
    build_live_report, grade_live_output, load_dataset, run_deterministic, ExpectedAction,
    LiveCaseResult, SubtitleEvalDataset,
};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

struct Options {
    dataset: PathBuf,
    config: Option<PathBuf>,
    report: Option<PathBuf>,
    live: bool,
    runs: usize,
}

#[tokio::main]
async fn main() {
    if let Err(error) = run().await {
        eprintln!("subtitle-eval failed: {error}");
        std::process::exit(1);
    }
}

async fn run() -> Result<(), String> {
    let options = parse_options()?;
    let dataset_json = fs::read_to_string(&options.dataset)
        .map_err(|error| format!("Cannot read {}: {error}", options.dataset.display()))?;
    let dataset = load_dataset(&dataset_json)?;
    let deterministic = run_deterministic(&dataset);
    println!(
        "{}",
        serde_json::to_string_pretty(&deterministic).map_err(|error| error.to_string())?
    );
    if !deterministic.passed {
        return Err("deterministic fixtures failed".to_string());
    }
    if !options.live {
        return Ok(());
    }

    let config_path = options
        .config
        .or_else(default_config_path)
        .ok_or_else(|| "Use --config <path> for live mode".to_string())?;
    let config_json = fs::read_to_string(&config_path)
        .map_err(|error| format!("Cannot read {}: {error}", config_path.display()))?;
    let mut app_config: AppConfig = serde_json::from_str(&config_json)
        .map_err(|error| format!("Invalid app config: {error}"))?;
    app_config.normalize();
    let mut backend_config = app_config.translation.foundry_local;
    let manifest = meowcal_sub::engine_manifest::EngineManifest::shipped()
        .map_err(|error| error.to_string())?;
    let model_id = backend_config
        .model
        .clone()
        .unwrap_or_else(|| manifest.model.id.clone());
    if let Some(runtime) = backend_config.managed_runtime.clone() {
        let endpoint =
            meowcal_sub::hy_mt_runtime::ensure_ready(&runtime, Duration::from_secs(90)).await?;
        backend_config.endpoint_url = Some(endpoint);
    }

    let backend = FoundryLocalBackend::new(backend_config);
    let result = run_live(
        &dataset,
        &backend,
        options.runs,
        manifest.engine_version,
        model_id,
    )
    .await;
    meowcal_sub::hy_mt_runtime::shutdown_owned();
    let report = result?;
    let report_json = serde_json::to_string_pretty(&report).map_err(|error| error.to_string())?;
    println!("{report_json}");
    if let Some(path) = options.report {
        write_report(&path, &report_json)?;
    }
    if !report.passed {
        return Err("one or more live subtitle cases failed".to_string());
    }
    Ok(())
}

async fn run_live(
    dataset: &SubtitleEvalDataset,
    backend: &FoundryLocalBackend,
    runs: usize,
    engine_version: String,
    model_id: String,
) -> Result<meowcal_sub::subtitle_eval::LiveEvalReport, String> {
    let mut results: Vec<LiveCaseResult> = Vec::new();
    for run in 1..=runs {
        for case in dataset
            .cases
            .iter()
            .filter(|case| case.expected_action == ExpectedAction::Translate)
        {
            let started = Instant::now();
            let output = backend
                .translate(
                    &case.source_text,
                    &case.source_language,
                    &case.target_language,
                )
                .await
                .map_err(|error| format!("{} run {run}: {error}", case.id))?;
            results.push(grade_live_output(
                case,
                run,
                &output,
                started.elapsed().as_millis() as u64,
            ));
        }
    }
    Ok(build_live_report(
        dataset,
        results,
        chrono::Utc::now().to_rfc3339(),
        env::consts::ARCH.to_string(),
        engine_version,
        model_id,
        runs,
    ))
}

fn parse_options() -> Result<Options, String> {
    let mut dataset = PathBuf::from("../evals/subtitle-eval-v1.json");
    let mut config = None;
    let mut report = None;
    let mut live = false;
    let mut runs = 1usize;
    let mut args = env::args().skip(1);
    while let Some(argument) = args.next() {
        match argument.as_str() {
            "--dataset" => dataset = PathBuf::from(next_value(&mut args, "--dataset")?),
            "--config" => config = Some(PathBuf::from(next_value(&mut args, "--config")?)),
            "--report" => report = Some(PathBuf::from(next_value(&mut args, "--report")?)),
            "--runs" => {
                runs = next_value(&mut args, "--runs")?
                    .parse()
                    .map_err(|_| "--runs must be a positive integer".to_string())?;
                if runs == 0 {
                    return Err("--runs must be a positive integer".to_string());
                }
            }
            "--live" => live = true,
            "--help" | "-h" => {
                println!(
                    "subtitle-eval [--dataset PATH] [--live] [--config PATH] [--runs N] [--report PATH]"
                );
                std::process::exit(0);
            }
            other => return Err(format!("Unknown argument: {other}")),
        }
    }
    Ok(Options {
        dataset,
        config,
        report,
        live,
        runs,
    })
}

fn next_value(args: &mut impl Iterator<Item = String>, name: &str) -> Result<String, String> {
    args.next()
        .ok_or_else(|| format!("{name} requires a value"))
}

fn default_config_path() -> Option<PathBuf> {
    env::var_os("APPDATA").map(|root| {
        PathBuf::from(root)
            .join("com.meowcal.sub")
            .join("config.json")
    })
}

fn write_report(path: &Path, json: &str) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("Cannot create {}: {error}", parent.display()))?;
    }
    fs::write(path, json).map_err(|error| format!("Cannot write {}: {error}", path.display()))
}
