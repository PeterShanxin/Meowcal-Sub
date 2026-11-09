//! Capture pacing calibrated from successful capture and OCR work.

use std::collections::VecDeque;
use std::time::{Duration, Instant};

/// Holds a capture loop to its current calibrated period.
#[derive(Debug, Clone)]
pub struct Pacer {
    period: Duration,
    configured: Duration,
    samples: VecDeque<Duration>,
    last_observed: Option<Instant>,
}

impl Pacer {
    pub fn new(interval_ms: u32) -> Self {
        Self {
            period: Duration::from_millis(interval_ms as u64),
            configured: Duration::from_millis(interval_ms.max(1) as u64),
            samples: VecDeque::new(),
            last_observed: None,
        }
    }

    /// Speed up inexpensive recognition within the configured capture cadence.
    /// Complex cues must not add idle time beyond that cadence: recognition
    /// overruns already slow sampling without an additional pacing penalty.
    pub fn observe_work(&mut self, work: Duration) {
        self.samples.push_back(work);
        if self.samples.len() > 24 {
            self.samples.pop_front();
        }
        if self.samples.len() < 12 {
            return;
        }
        let mut samples: Vec<_> = self.samples.iter().copied().collect();
        samples.sort_unstable();
        let p90 = samples[(samples.len() * 9).div_ceil(10) - 1];
        let minimum = (self.configured / 2).max(Duration::from_millis(60));
        let maximum = self.configured.max(minimum);
        let target = p90.saturating_mul(2).clamp(minimum, maximum);
        let step = (self.period / 8).max(Duration::from_millis(1));
        self.period = target.clamp(
            self.period.saturating_sub(step),
            self.period.saturating_add(step),
        );
    }

    /// Actual distance between successfully observed captures, including work
    /// overruns and recognition outages rather than only scheduled idle time.
    pub fn observed_interval(&mut self, started: Instant) -> Duration {
        self.last_observed
            .replace(started)
            .map_or(self.period, |previous| {
                started.saturating_duration_since(previous)
            })
    }

    /// One whole period, for a pass that has not started timing a frame yet.
    pub fn period(&self) -> Duration {
        self.period
    }

    /// How long to wait so the frame started at `frame_started` fills one period.
    pub fn remaining_for(&self, frame_started: Instant) -> Duration {
        remaining(self.period, frame_started.elapsed())
    }
}

/// How long to wait so a frame that started `elapsed` ago completes one `period`.
///
/// Returns zero when the frame already overran, which lets the loop start the
/// next capture immediately rather than falling a whole period further behind.
pub fn remaining(period: Duration, elapsed: Duration) -> Duration {
    period.saturating_sub(elapsed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn waits_out_the_rest_of_a_fast_frame() {
        let wait = remaining(Duration::from_millis(250), Duration::from_millis(40));
        assert_eq!(wait, Duration::from_millis(210));
    }

    // The frame that ran the translator is the one a viewer is waiting on, so it
    // must not also pay a full interval before the next capture.
    #[test]
    fn does_not_wait_at_all_after_a_slow_frame() {
        let wait = remaining(Duration::from_millis(250), Duration::from_millis(1800));
        assert_eq!(wait, Duration::ZERO);
    }

    #[test]
    fn a_frame_that_exactly_fills_the_period_waits_zero() {
        let wait = remaining(Duration::from_millis(250), Duration::from_millis(250));
        assert_eq!(wait, Duration::ZERO);
    }
}

#[cfg(test)]
mod calibration_tests {
    use super::*;
    #[test]
    fn fast_and_slow_devices_converge_to_different_bounded_periods() {
        for (work_ms, expected_ms) in [(40, 125), (100, 200), (400, 250), (900, 250)] {
            let mut pacer = Pacer::new(250);
            for _ in 0..80 {
                pacer.observe_work(Duration::from_millis(work_ms));
            }
            assert_eq!(pacer.period(), Duration::from_millis(expected_ms));
        }
    }
    #[test]
    fn startup_and_one_slow_frame_do_not_change_the_period() {
        let mut pacer = Pacer::new(250);
        for _ in 0..11 {
            pacer.observe_work(Duration::from_millis(40));
        }
        assert_eq!(pacer.period(), Duration::from_millis(250));
        for _ in 0..40 {
            pacer.observe_work(Duration::from_millis(40));
        }
        pacer.observe_work(Duration::from_secs(2));
        assert_eq!(pacer.period(), Duration::from_millis(125));
    }
    #[test]
    fn sustained_load_restores_the_configured_cadence_and_fast_frames_speed_it_up_again() {
        let mut pacer = Pacer::new(250);
        for _ in 0..40 {
            pacer.observe_work(Duration::from_millis(50));
        }
        for _ in 0..40 {
            pacer.observe_work(Duration::from_millis(300));
        }
        assert_eq!(pacer.period(), Duration::from_millis(250));
        for _ in 0..80 {
            pacer.observe_work(Duration::from_millis(50));
        }
        assert_eq!(pacer.period(), Duration::from_millis(125));
    }
}

#[cfg(test)]
mod overload_tests {
    use super::*;

    #[test]
    fn observed_intervals_include_work_and_missing_recognition_results() {
        let start = Instant::now();
        let mut pacer = Pacer::new(250);
        assert_eq!(pacer.observed_interval(start), Duration::from_millis(250));
        assert_eq!(
            pacer.observed_interval(start + Duration::from_millis(700)),
            Duration::from_millis(700)
        );
        assert_eq!(
            pacer.observed_interval(start + Duration::from_secs(4)),
            Duration::from_millis(3300)
        );
    }

    #[test]
    fn expensive_cues_do_not_add_idle_beyond_the_configured_capture_cadence() {
        let mut pacer = Pacer::new(250);
        for _ in 0..60 {
            pacer.observe_work(Duration::from_millis(500));
        }
        assert!(pacer.period() <= Duration::from_millis(250));
        assert_eq!(
            remaining(pacer.period(), Duration::from_millis(500)),
            Duration::ZERO
        );
    }
}
