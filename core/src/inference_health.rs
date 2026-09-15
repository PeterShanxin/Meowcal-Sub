//! Bounded evidence for inference correctness, independent of HTTP liveness.
use serde::Deserialize;
use serde_json::Value;
use std::collections::{hash_map::DefaultHasher, VecDeque};
use std::hash::{Hash, Hasher};
use std::time::{Duration, Instant};

pub const CAPABILITY: &str = "recoverInference";
pub const CPU_LOCK_EVENT: &str = "inferenceCpuLocked";

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Report {
    pub receipt: String,
    pub reason: Rejection,
}

#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Rejection {
    EmptyOutput,
    TooLong,
    RepetitionLoop,
    GarbageTail,
    PromptEcho,
    WrongLanguage,
}

struct Receipt {
    id: String,
    input: u64,
    capped: bool,
}

pub struct InferenceHealth {
    instance: String,
    sequence: u64,
    receipts: VecDeque<Receipt>,
    strong: VecDeque<(u64, Instant)>,
}

impl Default for InferenceHealth {
    fn default() -> Self {
        Self {
            instance: format!("{}-{:?}", std::process::id(), std::time::SystemTime::now()),
            sequence: 0,
            receipts: VecDeque::new(),
            strong: VecDeque::new(),
        }
    }
}

impl InferenceHealth {
    pub fn record(&mut self, request: &Value, response: &mut Value) {
        self.sequence += 1;
        let id = format!("{}-{}", self.instance, self.sequence);
        let mut hasher = DefaultHasher::new();
        request.get("messages").hash(&mut hasher);
        let capped = response
            .pointer("/choices/0/finish_reason")
            .and_then(Value::as_str)
            == Some("length");
        self.receipts.push_back(Receipt {
            id: id.clone(),
            input: hasher.finish(),
            capped,
        });
        if self.receipts.len() > 16 {
            self.receipts.pop_front();
        }
        response["inferenceReceipt"] = Value::String(id);
    }

    /// None means an old engine, duplicate report, or expired receipt.
    pub fn report(&mut self, report: &Report, now: Instant) -> Option<bool> {
        let position = self
            .receipts
            .iter()
            .position(|item| item.id == report.receipt)?;
        let receipt = self.receipts.remove(position)?;
        self.strong
            .retain(|(_, at)| now.saturating_duration_since(*at) <= Duration::from_secs(60));
        let strong = matches!(
            report.reason,
            Rejection::GarbageTail | Rejection::RepetitionLoop
        ) || (receipt.capped && matches!(report.reason, Rejection::TooLong));
        if strong && !self.strong.iter().any(|(input, _)| *input == receipt.input) {
            self.strong.push_back((receipt.input, now));
        }
        Some(self.strong.len() >= 2)
    }

    pub fn reset_engine(&mut self) {
        self.receipts.clear();
        self.strong.clear();
    }
}

/// Reject generated symbol debris; preserve punctuation, names, and source code.
pub fn has_garbage_tail(source: &str, output: &str) -> bool {
    let tail = output
        .trim()
        .trim_end_matches(['"', '\'', '”', '’'])
        .rsplit(|ch: char| ch.is_alphanumeric() || ch.is_whitespace())
        .next()
        .unwrap_or_default();
    if tail.chars().count() < 4 || source.contains(tail) {
        return false;
    }
    let abnormal = tail
        .chars()
        .filter(|ch| matches!(ch, '+' | '&' | '$' | '=' | '_' | '{' | '}' | '\\' | '|'))
        .count();
    abnormal > 0
        && tail
            .chars()
            .filter(|ch| matches!(ch, '-' | '.' | '+' | '&' | '$' | '=' | '_'))
            .count()
            >= 4
}

pub fn has_cjk_loop(source: &str, output: &str) -> bool {
    let chars: Vec<char> = output.chars().collect();
    if chars.len() < source.chars().count().saturating_mul(2).max(12)
        || !chars.iter().any(|ch| {
            matches!(ch,
                '\u{3400}'..='\u{9fff}' | '\u{3040}'..='\u{30ff}' |
                '\u{31f0}'..='\u{31ff}' | '\u{ff66}'..='\u{ff9f}' |
                '\u{1100}'..='\u{11ff}' | '\u{3130}'..='\u{318f}' |
                '\u{a960}'..='\u{a97f}' | '\u{ac00}'..='\u{d7ff}')
        })
    {
        return false;
    }
    (1..=8).any(|width| {
        chars.windows(width * 6).any(|window| {
            window
                .chunks_exact(width)
                .all(|chunk| chunk == &window[..width])
                && !source.contains(&window.iter().collect::<String>())
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn rejects_observed_tails_and_cjk_loops_without_trimming_translation() {
        for output in [
            "最后一班渡轮会在中午之前离开。---+",
            "那个小镇仍然处于L状态......-.-..&.-.",
        ] {
            assert!(has_garbage_tail(
                "The last ferry leaves before sunrise.",
                output
            ));
        }
        for output in [
            "真的……？！",
            "等一下---",
            "C++",
            "Jean-Luc",
            "Imageong • 设置 — 更新",
            "x = y + z",
            "你好 :-)",
        ] {
            assert!(!has_garbage_tail("hello", output), "{output}");
        }
        assert!(!has_garbage_tail("Print ---+", "输出 ---+"));
        assert!(has_cjk_loop("hello", "你好你好你好你好你好你好"));
        assert!(!has_cjk_loop("哈哈哈哈哈哈哈哈", "哈哈哈哈哈哈哈哈"));
    }

    #[test]
    fn repetition_covers_kana_and_hangul_with_source_controls() {
        for unit in ["あ", "ア", "ｱ", "한", "ᄀ", "ㄱ"] {
            let output = unit.repeat(12);
            assert!(has_cjk_loop("hello", &output), "{output}");
            assert!(
                !has_cjk_loop(&output, &output),
                "source repetition: {output}"
            );
        }
        for output in ["ちょっと待ってください。", "잠시 기다려 주세요."] {
            assert!(!has_cjk_loop("wait", output), "{output}");
        }
    }

    fn record(health: &mut InferenceHealth, source: &str, reason: Rejection) -> Report {
        let mut response = json!({"choices":[{"finish_reason":"length"}]});
        health.record(&json!({"messages":[{"content":source}]}), &mut response);
        Report {
            receipt: response["inferenceReceipt"].as_str().unwrap().into(),
            reason,
        }
    }

    #[test]
    fn counts_independent_strong_requests_and_rejects_stale_reports() {
        let mut health = InferenceHealth::default();
        let now = Instant::now();
        let first = record(&mut health, "one", Rejection::TooLong);
        assert_eq!(health.report(&first, now), Some(false));
        assert_eq!(health.report(&first, now), None);
        let duplicate = record(&mut health, "one", Rejection::TooLong);
        assert_eq!(health.report(&duplicate, now), Some(false));
        let second = record(&mut health, "two", Rejection::GarbageTail);
        assert_eq!(health.report(&second, now), Some(true));
        let old = record(&mut health, "three", Rejection::TooLong);
        health.reset_engine();
        assert_eq!(health.report(&old, now), None);
    }

    #[test]
    fn weak_signals_and_old_strong_signals_do_not_force_cpu() {
        let mut health = InferenceHealth::default();
        let now = Instant::now();
        let weak = record(&mut health, "one", Rejection::WrongLanguage);
        assert_eq!(health.report(&weak, now), Some(false));
        let first = record(&mut health, "two", Rejection::GarbageTail);
        assert_eq!(health.report(&first, now), Some(false));
        let later = record(&mut health, "three", Rejection::GarbageTail);
        assert_eq!(
            health.report(&later, now + Duration::from_secs(61)),
            Some(false)
        );
    }
}
