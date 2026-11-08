use crate::pipeline_session::PipelineToken;
use crate::subtitle_queue::{Completion, Display, Frame, Presentation, SubtitleQueue};
use crate::{current_band, legacy_band, ocr_recent_lines, ocr_stability, pipeline_pacing::Pacer};
use std::collections::HashSet;
use std::time::{Duration, Instant};

struct Segment {
    text: String,
    start: u64,
    end: u64,
}
struct Case {
    name: &'static str,
    cues: Vec<Segment>,
    jitter: bool,
    busy: bool,
    overload: bool,
    target: &'static str,
}
struct Metrics {
    seen: Vec<(usize, u64)>,
    clears: Vec<u64>,
    samples: u64,
    max_depth: usize,
    skipped: u64,
    final_depth: usize,
}

fn case(name: &'static str, texts: &[&str], period: u64, count: usize) -> Case {
    Case {
        name,
        cues: (0..count)
            .map(|i| Segment {
                text: texts[i % texts.len()].into(),
                start: i as u64 * period,
                end: (i as u64 + 1) * period,
            })
            .collect(),
        jitter: false,
        busy: false,
        overload: false,
        target: "稍等。",
    }
}

fn latency(case: &Case, id: usize, started: u64) -> u64 {
    if case.overload && started < 21_000 {
        4_000
    } else if case.busy && id == 0 {
        1_800
    } else {
        [400, 450, 650, 350, 500][id % 5]
    }
}

fn source(case: &Case, at: u64, sample: u64) -> Option<(usize, String)> {
    case.cues
        .iter()
        .enumerate()
        .find(|(_, cue)| cue.start <= at && at < cue.end)
        .map(|(id, cue)| {
            let text = if case.jitter && sample % 7 == 3 {
                format!("{} X", cue.text)
            } else {
                cue.text.clone()
            };
            (id, text)
        })
}

fn metrics() -> Metrics {
    Metrics {
        seen: vec![],
        clears: vec![],
        samples: 0,
        max_depth: 0,
        skipped: 0,
        final_depth: 0,
    }
}

fn old(case: &Case, start: Instant) -> Metrics {
    let mut metrics = metrics();
    let mut band = legacy_band::Gate::default();
    let mut recent = ocr_recent_lines::RecentLines::new();
    let mut slot: Option<(usize, u64)> = None;
    let mut visible = false;
    let mut empty = 0;
    let finish = case.cues.last().unwrap().end + 10_000;
    for at in (0..=finish).step_by(25) {
        if slot.is_some_and(|(_, due)| at >= due) {
            let (id, _) = slot.take().unwrap();
            metrics.seen.push((id, at));
            visible = true;
        }
        if at % 250 != 0 {
            continue;
        }
        metrics.samples += 1;
        let reading = source(case, at, metrics.samples);
        let text = reading.as_ref().map_or("", |(_, text)| text.as_str());
        let admitted = !text.is_empty() && band.observe(text, at);
        if !admitted {
            empty += 1;
            if empty >= 3 && slot.is_none() && visible {
                metrics.clears.push(at);
                visible = false;
                recent.clear();
            }
            continue;
        }
        empty = 0;
        // Moderate was the installed default: fewer than two significant
        // characters never reached the old translator.
        if text.chars().filter(|ch| ch.is_alphanumeric()).count() < 2 {
            continue;
        }
        let now = start + Duration::from_millis(at);
        if recent.classify(text, now) == ocr_stability::LineChange::Repeat || slot.is_some() {
            continue;
        }
        let id = reading.as_ref().unwrap().0;
        recent.remember(text, now);
        slot = Some((id, at + latency(case, id, at)));
        metrics.max_depth = 1;
    }
    metrics.final_depth = usize::from(slot.is_some());
    metrics
}

fn current(case: &Case, start: Instant) -> Metrics {
    let mut metrics = metrics();
    let mut band = current_band::Gate::default();
    let mut queue = SubtitleQueue::<usize>::new(Duration::from_millis(250));
    let mut slot = None;
    let mut next_sample = 0;
    let mut pacer = Pacer::new(250);
    let finish = case.cues.last().unwrap().end + 12_000;
    for at in (0..=finish).step_by(25) {
        let now = start + Duration::from_millis(at);
        if let Some((job, id, due)) = slot.take() {
            if at >= due {
                queue.complete(
                    &job,
                    Completion {
                        result: id,
                        text: case.target.into(),
                        presentation: Presentation::Translated,
                        retryable: false,
                    },
                    now,
                );
            } else {
                slot = Some((job, id, due));
            }
        }
        if at >= next_sample {
            metrics.samples += 1;
            let reading = source(case, at, metrics.samples);
            let text = reading.as_ref().map_or("", |(_, text)| text.as_str());
            let eligible = !text.is_empty() && band.observe(text, at);
            queue.observe(
                Frame {
                    token: PipelineToken {
                        session_id: 1,
                        capture_id: metrics.samples,
                    },
                    region_generation: 0,
                    text: text.into(),
                    context_prompt: None,
                    started: now,
                    capture_ms: 5,
                    ocr_ms: 45,
                    sample_ms: pacer.period().as_millis() as u64,
                },
                eligible,
                now,
            );
            pacer.observe_work(Duration::from_millis(50));
            next_sample = at + pacer.period().as_millis() as u64;
        }
        if slot.is_none() {
            if let Some(job) = queue.next_job(now) {
                let captured = job.frame.started.duration_since(start).as_millis() as u64;
                let id = case
                    .cues
                    .iter()
                    .position(|cue| cue.start <= captured && captured < cue.end)
                    .unwrap();
                slot = Some((job, id, at + latency(case, id, at)));
            }
        }
        if let Some(action) = queue.next_display(now) {
            match action {
                Display::Show { result, .. } => metrics.seen.push((result, at)),
                Display::Clear => metrics.clears.push(at),
            }
        }
        let stats = queue.stats(now);
        metrics.max_depth = metrics.max_depth.max(stats.depth);
        metrics.skipped = stats.skipped;
        metrics.final_depth = stats.depth;
    }
    metrics
}

fn report(case: &Case, kind: &str, metrics: &Metrics) -> (String, bool) {
    let distinct: HashSet<_> = metrics.seen.iter().map(|(id, _)| *id).collect();
    let missed = case.cues.len() - distinct.len();
    let duplicates = metrics.seen.len() - distinct.len();
    let reordered = metrics
        .seen
        .windows(2)
        .filter(|pair| pair[0].0 >= pair[1].0)
        .count();
    let mut latencies: Vec<_> = metrics
        .seen
        .iter()
        .map(|(id, at)| at.saturating_sub(case.cues[*id].start))
        .collect();
    latencies.sort_unstable();
    let quantile = |p: usize| {
        if latencies.is_empty() {
            0
        } else {
            latencies[(latencies.len() * p).div_ceil(100).saturating_sub(1)]
        }
    };
    let mut dwell = vec![];
    for (i, (_, at)) in metrics.seen.iter().enumerate() {
        let next = metrics
            .seen
            .get(i + 1)
            .map(|(_, t)| *t)
            .into_iter()
            .chain(metrics.clears.iter().copied().filter(|t| t >= at))
            .min();
        if let Some(end) = next {
            dwell.push(end - at);
        }
    }
    let min_visible = dwell.into_iter().min().unwrap_or(0);
    let passed = if case.overload {
        metrics.skipped > 0 && metrics.final_depth == 0 && distinct.contains(&(case.cues.len() - 1))
    } else {
        missed == 0
            && duplicates == 0
            && reordered == 0
            && metrics.skipped == 0
            && metrics.final_depth == 0
            && metrics.max_depth <= 4
            && min_visible >= 350
    };
    (format!("{{\"case\":\"{}\",\"policy\":\"{}\",\"cues\":{},\"shown\":{},\"missed\":{},\"duplicates\":{},\"reordered\":{},\"p50_ms\":{},\"p95_ms\":{},\"min_visible_ms\":{},\"max_depth\":{},\"final_depth\":{},\"skipped\":{},\"ocr_samples\":{},\"passed\":{}}}",
        case.name,kind,case.cues.len(),metrics.seen.len(),missed,duplicates,reordered,quantile(50),quantile(95),min_visible,metrics.max_depth,metrics.final_depth,metrics.skipped,metrics.samples,passed),passed)
}

pub fn run() {
    let mut cases = vec![
        case(
            "rapid-600ms",
            &["Where?", "Inside.", "Hurry!", "Wait.", "Why?", "Go!"],
            600,
            120,
        ),
        case(
            "rapid-800ms",
            &["Where?", "Inside.", "Hurry!", "Wait.", "Why?", "Go!"],
            800,
            120,
        ),
        case(
            "rapid-1000ms",
            &["Where?", "Inside.", "Hurry!", "Wait.", "Why?", "Go!"],
            1000,
            120,
        ),
        case(
            "negations",
            &["I can", "I can't", "I can", "I can't", "我能去", "我不能去"],
            1500,
            30,
        ),
        case("single-cjk", &["好", "不", "走", "嗯", "来", "停"], 900, 36),
        case(
            "returning-dialogue",
            &["Yes", "No", "Yes", "Wait", "No", "Wait"],
            1500,
            30,
        ),
        case(
            "ocr-jitter",
            &[
                "Please wait by the entrance.",
                "The morning train is arriving.",
            ],
            2500,
            30,
        ),
        case(
            "paused-source",
            &["Please wait here.", "The train is ready."],
            30000,
            2,
        ),
        case(
            "busy-translator",
            &["Where?", "Inside.", "Hurry!", "Wait.", "Why?", "Go!"],
            800,
            60,
        ),
        case("late-result", &["Please wait for me."], 800, 1),
        case(
            "severe-stall-recovery",
            &["Where?", "Inside.", "Hurry!", "Wait.", "Why?", "Go!"],
            800,
            60,
        ),
    ];
    let mut numbers = case("changed-numbers", &["room"], 1200, 40);
    for (i, cue) in numbers.cues.iter_mut().enumerate() {
        cue.text = format!("Please wait in room {}", i + 10);
    }
    numbers.target = "请在十二号房间等我。";
    cases[6].target = "请在车站入口等候，最后一班列车即将到达。";
    cases.push(numbers);
    cases.push(case(
        "rapid-500ms",
        &["Where?", "Inside.", "Hurry!", "Wait.", "Why?", "Go!"],
        500,
        120,
    ));
    cases[6].jitter = true;
    cases[8].busy = true;
    cases[9].busy = true;
    cases[10].overload = true;
    let start = Instant::now();
    let mut rows = vec![];
    let mut passed = true;
    for case in &cases {
        rows.push(report(case, "baseline", &old(case, start)).0);
        let (row, ok) = report(case, "fifo", &current(case, start));
        passed &= ok;
        rows.push(row);
    }
    println!("{{\"schema_version\":1,\"model\":\"deterministic timing simulation; no inference or native OCR\",\"passed\":{},\"results\":[{}]}}",passed,rows.join(","));
    if !passed {
        std::process::exit(1);
    }
}
