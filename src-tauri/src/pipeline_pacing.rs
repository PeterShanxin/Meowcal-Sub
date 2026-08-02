// =============================================================================
// PIPELINE PACING - keeping the capture loop on a fixed period
// =============================================================================
// The capture loop used to finish a frame and then sleep a flat interval, so
// the real period was `work + interval` rather than `interval`. On a frame that
// ran the translator - the only frames a viewer notices - that added the whole
// interval on top of a second or more of model time, and pushed the next
// capture that far past the moment the next subtitle appeared.
//
// Pacing to a deadline instead means a slow frame costs its own time and
// nothing more: the loop is ready for the next subtitle as soon as it is done.
// =============================================================================

use std::time::{Duration, Instant};

/// Holds a capture loop to a fixed period.
#[derive(Debug, Clone, Copy)]
pub struct Pacer {
    period: Duration,
}

impl Pacer {
    pub fn new(interval_ms: u32) -> Self {
        Self {
            period: Duration::from_millis(interval_ms as u64),
        }
    }

    /// One whole period, for a pass that has not started timing a frame yet.
    pub fn period(self) -> Duration {
        self.period
    }

    /// How long to wait so the frame started at `frame_started` fills one period.
    pub fn remaining_for(self, frame_started: Instant) -> Duration {
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
