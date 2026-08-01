use crate::llm::{
    is_untranslatable_text, output_validation::validate_translation_output,
    output_validation::TranslationOutputRejection,
};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

pub const SUPPORTED_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubtitleEvalDataset {
    pub schema_version: u32,
    pub dataset_id: String,
    pub provenance: String,
    pub cases: Vec<SubtitleEvalCase>,
    pub rejection_fixtures: Vec<RejectionFixture>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubtitleEvalCase {
    pub id: String,
    pub source_language: String,
    pub target_language: String,
    pub source_text: String,
    pub tags: Vec<String>,
    pub expected_action: ExpectedAction,
    pub acceptable_outputs: Vec<String>,
    pub max_output_lines: usize,
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ExpectedAction {
    Translate,
    Filter,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RejectionFixture {
    pub id: String,
    pub source_language: String,
    pub target_language: String,
    pub source_text: String,
    pub output: String,
    pub expected_reason: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeterministicEvalReport {
    pub dataset_id: String,
    pub passed: bool,
    pub checks: usize,
    pub failures: Vec<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LiveCaseResult {
    pub case_id: String,
    pub run: usize,
    pub passed: bool,
    pub latency_ms: u64,
    pub output_chars: usize,
    pub output_lines: usize,
    pub latin_letters: usize,
    pub validator_decision: String,
    pub reason: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LiveEvalReport {
    pub schema_version: u32,
    pub dataset_id: String,
    pub generated_at_utc: String,
    pub architecture: String,
    pub engine_version: String,
    pub model_id: String,
    pub runs: usize,
    /// Latency of the fixed sample request used to warm the runtime before
    /// measured subtitle requests begin. This is reported separately so the
    /// warm-model budgets are not confused with first-load latency.
    pub warmup_latency_ms: u64,
    pub warmup_passed: bool,
    pub passed: bool,
    pub translated_attempts: usize,
    pub filtered_cases: usize,
    pub p50_latency_ms: u64,
    pub p95_latency_ms: u64,
    pub p50_within_budget: bool,
    pub p95_within_budget: bool,
    pub results: Vec<LiveCaseResult>,
}

#[derive(Debug, Clone)]
pub struct LiveEvalMetadata {
    pub generated_at_utc: String,
    pub architecture: String,
    pub engine_version: String,
    pub model_id: String,
    pub runs: usize,
    pub warmup_latency_ms: u64,
    pub warmup_passed: bool,
}

pub fn load_dataset(json: &str) -> Result<SubtitleEvalDataset, String> {
    let dataset: SubtitleEvalDataset =
        serde_json::from_str(json).map_err(|error| format!("Invalid eval JSON: {error}"))?;
    validate_dataset_shape(&dataset)?;
    Ok(dataset)
}

pub fn run_deterministic(dataset: &SubtitleEvalDataset) -> DeterministicEvalReport {
    let mut checks = 0;
    let mut failures = Vec::new();
    for case in &dataset.cases {
        let filtered = is_untranslatable_text(&case.source_text);
        checks += 1;
        match case.expected_action {
            ExpectedAction::Filter if !filtered => {
                failures.push(format!("{}: expected source filter", case.id));
            }
            ExpectedAction::Translate if filtered => {
                failures.push(format!("{}: valid subtitle was filtered", case.id));
            }
            _ => {}
        }
        if case.expected_action == ExpectedAction::Translate {
            for output in &case.acceptable_outputs {
                checks += 1;
                if let Err(reason) = validate_translation_output(
                    &case.source_text,
                    output,
                    &case.source_language,
                    &case.target_language,
                ) {
                    failures.push(format!(
                        "{}: acceptable output rejected as {}",
                        case.id,
                        reason.code()
                    ));
                }
            }
        }
    }
    for fixture in &dataset.rejection_fixtures {
        checks += 1;
        match validate_translation_output(
            &fixture.source_text,
            &fixture.output,
            &fixture.source_language,
            &fixture.target_language,
        ) {
            Err(reason) if reason.code() == fixture.expected_reason => {}
            Err(reason) => failures.push(format!(
                "{}: expected {}, got {}",
                fixture.id,
                fixture.expected_reason,
                reason.code()
            )),
            Ok(()) => failures.push(format!(
                "{}: expected rejection {}",
                fixture.id, fixture.expected_reason
            )),
        }
    }
    DeterministicEvalReport {
        dataset_id: dataset.dataset_id.clone(),
        passed: failures.is_empty(),
        checks,
        failures,
    }
}

pub fn grade_live_output(
    case: &SubtitleEvalCase,
    run: usize,
    output: &str,
    latency_ms: u64,
) -> LiveCaseResult {
    let trimmed = output.trim();
    let output_lines = trimmed.lines().count();
    let latin_letters = trimmed
        .chars()
        .filter(|character| character.is_ascii_alphabetic())
        .count();
    let decision = validate_translation_output(
        &case.source_text,
        trimmed,
        &case.source_language,
        &case.target_language,
    );
    let shape_reason = if trimmed == case.source_text.trim() {
        Some("source_passthrough")
    } else if output_lines > case.max_output_lines {
        Some("too_many_lines")
    } else if latin_letters == 0 {
        Some("no_latin_output")
    } else {
        None
    };
    let validator_reason = decision.err().map(TranslationOutputRejection::code);
    let reason = shape_reason.or(validator_reason).map(str::to_string);
    LiveCaseResult {
        case_id: case.id.clone(),
        run,
        passed: reason.is_none(),
        latency_ms,
        output_chars: trimmed.chars().count(),
        output_lines,
        latin_letters,
        validator_decision: if validator_reason.is_none() {
            "accepted"
        } else {
            "rejected"
        }
        .to_string(),
        reason,
    }
}

pub fn build_live_report(
    dataset: &SubtitleEvalDataset,
    results: Vec<LiveCaseResult>,
    metadata: LiveEvalMetadata,
) -> LiveEvalReport {
    let mut latencies: Vec<u64> = results.iter().map(|result| result.latency_ms).collect();
    latencies.sort_unstable();
    let p50_latency_ms = percentile(&latencies, 50);
    let p95_latency_ms = percentile(&latencies, 95);
    LiveEvalReport {
        schema_version: 1,
        dataset_id: dataset.dataset_id.clone(),
        generated_at_utc: metadata.generated_at_utc,
        architecture: metadata.architecture,
        engine_version: metadata.engine_version,
        model_id: metadata.model_id,
        runs: metadata.runs,
        warmup_latency_ms: metadata.warmup_latency_ms,
        warmup_passed: metadata.warmup_passed,
        passed: results.iter().all(|result| result.passed),
        translated_attempts: results.len(),
        filtered_cases: dataset
            .cases
            .iter()
            .filter(|case| case.expected_action == ExpectedAction::Filter)
            .count(),
        p50_latency_ms,
        p95_latency_ms,
        p50_within_budget: p50_latency_ms <= 800,
        p95_within_budget: p95_latency_ms <= 1_800,
        results,
    }
}

fn percentile(sorted: &[u64], percentile: usize) -> u64 {
    if sorted.is_empty() {
        return 0;
    }
    let index = (sorted.len() * percentile).div_ceil(100).saturating_sub(1);
    sorted[index.min(sorted.len() - 1)]
}

fn validate_dataset_shape(dataset: &SubtitleEvalDataset) -> Result<(), String> {
    if dataset.schema_version != SUPPORTED_SCHEMA_VERSION {
        return Err(format!(
            "Unsupported eval schema {}",
            dataset.schema_version
        ));
    }
    if dataset.dataset_id.trim().is_empty() || dataset.provenance.trim().is_empty() {
        return Err("Eval dataset requires ID and provenance".to_string());
    }
    let mut ids = HashSet::new();
    for case in &dataset.cases {
        if !ids.insert(case.id.as_str()) {
            return Err(format!("Duplicate eval case ID: {}", case.id));
        }
        if case.tags.is_empty() {
            return Err(format!("{}: at least one tag required", case.id));
        }
        match case.expected_action {
            ExpectedAction::Translate if case.acceptable_outputs.is_empty() => {
                return Err(format!("{}: acceptable output required", case.id));
            }
            ExpectedAction::Filter if !case.acceptable_outputs.is_empty() => {
                return Err(format!("{}: filtered case cannot define output", case.id));
            }
            _ => {}
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shipped_dataset() -> SubtitleEvalDataset {
        load_dataset(include_str!("../../evals/subtitle-eval-v1.json")).unwrap()
    }

    #[test]
    fn shipped_dataset_passes_deterministic_eval() {
        let report = run_deterministic(&shipped_dataset());
        assert!(report.passed, "{:?}", report.failures);
        assert!(report.checks >= 25);
    }

    #[test]
    fn live_grader_rejects_passthrough_and_bad_shape() {
        let dataset = shipped_dataset();
        let case = &dataset.cases[0];
        assert_eq!(
            grade_live_output(case, 1, &case.source_text, 10)
                .reason
                .as_deref(),
            Some("source_passthrough")
        );
        assert_eq!(
            grade_live_output(case, 1, "One.\nTwo.", 10)
                .reason
                .as_deref(),
            Some("too_many_lines")
        );
    }

    #[test]
    fn latency_percentiles_use_nearest_rank() {
        assert_eq!(percentile(&[100, 200, 300, 400], 50), 200);
        assert_eq!(percentile(&[100, 200, 300, 400], 95), 400);
    }

    #[test]
    fn live_report_keeps_warmup_separate_from_measured_attempts() {
        let dataset = shipped_dataset();
        let case = &dataset.cases[0];
        let result = grade_live_output(case, 1, &case.acceptable_outputs[0], 700);
        let report = build_live_report(
            &dataset,
            vec![result],
            LiveEvalMetadata {
                generated_at_utc: "now".to_string(),
                architecture: "aarch64".to_string(),
                engine_version: "1.0.1".to_string(),
                model_id: "HY-MT".to_string(),
                runs: 1,
                warmup_latency_ms: 2_000,
                warmup_passed: true,
            },
        );
        assert_eq!(report.warmup_latency_ms, 2_000);
        assert!(report.warmup_passed);
        assert_eq!(report.p50_latency_ms, 700);
    }
}
