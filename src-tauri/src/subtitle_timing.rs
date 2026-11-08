//! Reading time starts when translated text becomes visible, including late results.

use std::time::Duration;

pub fn reading_time(source_ms: u64, translated: &str, delay_ms: u64) -> Duration {
    let chars = translated.chars().count() as u64;
    let cjk = translated
        .chars()
        .filter(|c| crate::llm::text_utils::is_cjk_char(*c))
        .count() as u64;
    let reading_ms = (400 + (chars - cjk) * 35 + cjk * 70).clamp(600, 3_500);
    // Brief replies need less time than a sentence. A fixed display floor
    // would cap throughput even when inference keeps up with fast dialogue.
    let floor_ms = (250 + (chars - cjk) * 25 + cjk * 50).clamp(350, 1_200);
    let hold_ms = source_ms
        .max(reading_ms)
        .saturating_sub(delay_ms)
        .clamp(floor_ms, 4_000);
    Duration::from_millis(hold_ms)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_late_result_still_has_time_to_be_read() {
        assert_eq!(reading_time(800, "Go!", 2_000), Duration::from_millis(350));
        assert!(
            reading_time(800, "Please wait for me outside.", 2_000) >= Duration::from_millis(800)
        );
    }

    #[test]
    fn duration_length_and_delay_change_the_display_budget() {
        assert!(reading_time(3_000, "Yes", 500) > reading_time(1_000, "Yes", 500));
        assert!(
            reading_time(800, "Please wait for me outside.", 500) > reading_time(800, "Yes", 500)
        );
        assert!(reading_time(3_000, "Yes", 1_500) < reading_time(3_000, "Yes", 500));
        assert!(reading_time(u64::MAX, "Yes", 0) <= Duration::from_secs(4));
    }
}
