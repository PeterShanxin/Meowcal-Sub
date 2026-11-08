//! User-facing waiting state, independent of subtitle retirement and deduplication.

use std::time::{Duration, Instant};

const WAIT_AFTER: Duration = Duration::from_secs(15);
const OCR_FAILURE_NOTICE_FRAMES: u32 = 3;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActivityState {
    Active,
    Waiting,
}

impl ActivityState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Waiting => "waiting",
        }
    }
}

pub struct SubtitleActivity {
    empty_since: Option<Instant>,
    last_observed: Option<Instant>,
    showing: Option<ActivityState>,
    waiting: bool,
    max_gap: Duration,
    ocr_failures: u32,
}

impl SubtitleActivity {
    pub fn new(interval: Duration) -> Self {
        Self {
            empty_since: None,
            last_observed: None,
            showing: None,
            waiting: false,
            max_gap: Duration::from_secs(2).max(interval.saturating_mul(2)),
            ocr_failures: 0,
        }
    }

    /// Call on successful OCR before filtering or deduplication. Unobserved time
    /// must not turn a capture outage or suspended PC into a healthy quiet region.
    pub fn observe(&mut self, has_text: bool, now: Instant) -> Option<ActivityState> {
        if self
            .last_observed
            .is_some_and(|last| now.saturating_duration_since(last) > self.max_gap)
        {
            self.interrupt();
        }
        self.last_observed = Some(now);
        self.ocr_failures = 0;
        let state = if has_text {
            self.empty_since = None;
            self.waiting = false;
            ActivityState::Active
        } else {
            let since = self.empty_since.get_or_insert(now);
            self.waiting |= now.saturating_duration_since(*since) >= WAIT_AFTER;
            if self.waiting {
                ActivityState::Waiting
            } else {
                ActivityState::Active
            }
        };
        if self.showing == Some(state) {
            return None;
        }
        self.showing = Some(state);
        Some(state)
    }

    pub fn interrupt(&mut self) {
        // An outage resets an unfinished wait, but only source text ends a shown wait.
        self.empty_since = None;
        self.last_observed = None;
        self.showing = None;
    }

    /// A single failed read is retried quietly. A sustained failure reports once
    /// until a successful observation proves that recognition recovered.
    pub fn recognition_failed(&mut self) -> bool {
        self.interrupt();
        self.ocr_failures = self.ocr_failures.saturating_add(1);
        self.ocr_failures == OCR_FAILURE_NOTICE_FRAMES
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tracker() -> SubtitleActivity {
        SubtitleActivity::new(Duration::from_millis(250))
    }

    fn empty_through(activity: &mut SubtitleActivity, start: Instant, seconds: u64) {
        for second in 0..seconds {
            activity.observe(false, start + Duration::from_secs(second));
        }
    }

    #[test]
    fn healthy_empty_region_waits_at_fifteen_seconds_and_only_once() {
        let mut activity = tracker();
        let start = Instant::now();
        assert_eq!(activity.observe(false, start), Some(ActivityState::Active));
        for second in 1..15 {
            assert_eq!(
                activity.observe(false, start + Duration::from_secs(second)),
                None
            );
        }
        assert_eq!(
            activity.observe(false, start + Duration::from_millis(14_999)),
            None
        );
        assert_eq!(
            activity.observe(false, start + Duration::from_secs(15)),
            Some(ActivityState::Waiting)
        );
        assert_eq!(
            activity.observe(false, start + Duration::from_secs(16)),
            None
        );
    }

    #[test]
    fn any_text_resets_waiting_even_if_the_line_will_be_deduplicated() {
        let mut activity = tracker();
        let start = Instant::now();
        empty_through(&mut activity, start, 16);
        let resumed = start + Duration::from_secs(16);
        assert_eq!(activity.observe(true, resumed), Some(ActivityState::Active));
        assert_eq!(
            activity.observe(true, resumed + Duration::from_secs(1)),
            None
        );
        assert_eq!(
            activity.observe(false, resumed + Duration::from_secs(2)),
            None
        );
        for second in 3..17 {
            assert_eq!(
                activity.observe(false, resumed + Duration::from_secs(second)),
                None
            );
        }
        assert_eq!(
            activity.observe(false, resumed + Duration::from_secs(17)),
            Some(ActivityState::Waiting)
        );
    }

    #[test]
    fn capture_and_ocr_outages_do_not_count_toward_waiting() {
        for recognition_failure in [false, true] {
            let mut activity = tracker();
            let start = Instant::now();
            empty_through(&mut activity, start, 14);
            if recognition_failure {
                assert!(!activity.recognition_failed());
            } else {
                activity.interrupt();
            }
            assert_eq!(
                activity.observe(false, start + Duration::from_secs(15)),
                Some(ActivityState::Active)
            );
        }
    }

    #[test]
    fn an_existing_wait_survives_outages_until_source_text_returns() {
        for recognition_failure in [false, true] {
            let mut activity = tracker();
            let start = Instant::now();
            empty_through(&mut activity, start, 16);
            if recognition_failure {
                assert!(!activity.recognition_failed());
            } else {
                activity.interrupt();
            }
            assert_eq!(
                activity.observe(false, start + Duration::from_secs(30)),
                Some(ActivityState::Waiting)
            );
            // Another unobserved gap must not dismiss the existing wait either.
            assert_eq!(
                activity.observe(false, start + Duration::from_secs(60)),
                Some(ActivityState::Waiting)
            );
            assert_eq!(
                activity.observe(true, start + Duration::from_secs(61)),
                Some(ActivityState::Active)
            );
            empty_through(&mut activity, start + Duration::from_secs(62), 15);
            assert_eq!(
                activity.observe(false, start + Duration::from_secs(77)),
                Some(ActivityState::Waiting)
            );
        }
    }

    #[test]
    fn an_unobserved_gap_restarts_the_clock_and_a_slow_configured_interval_is_allowed() {
        let start = Instant::now();
        let mut activity = tracker();
        activity.observe(false, start);
        assert_eq!(
            activity.observe(false, start + WAIT_AFTER),
            Some(ActivityState::Active)
        );
        let mut slow = SubtitleActivity::new(Duration::from_secs(5));
        slow.observe(false, start);
        assert_eq!(slow.observe(false, start + Duration::from_secs(5)), None);
        assert_eq!(slow.observe(false, start + Duration::from_secs(10)), None);
        assert_eq!(
            slow.observe(false, start + WAIT_AFTER),
            Some(ActivityState::Waiting)
        );
    }

    #[test]
    fn recognition_errors_are_quiet_until_the_threshold_and_recover_after_success() {
        let mut activity = tracker();
        assert!(!activity.recognition_failed());
        assert!(!activity.recognition_failed());
        assert!(activity.recognition_failed());
        assert!(!activity.recognition_failed());
        assert_eq!(
            activity.observe(true, Instant::now()),
            Some(ActivityState::Active)
        );
        assert!(!activity.recognition_failed());
        assert!(!activity.recognition_failed());
        assert!(activity.recognition_failed());
    }
}
