use std::sync::atomic::{AtomicU64, Ordering};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PipelineToken {
    pub session_id: u64,
    pub capture_id: u64,
}

#[derive(Debug, Default)]
pub struct PipelineClock {
    session_id: AtomicU64,
    capture_id: AtomicU64,
    translation_id: AtomicU64,
}

impl PipelineClock {
    pub fn begin_session(&self) -> u64 {
        self.capture_id.store(0, Ordering::SeqCst);
        self.translation_id.store(0, Ordering::SeqCst);
        self.session_id.fetch_add(1, Ordering::SeqCst) + 1
    }

    pub fn next_capture(&self, session_id: u64) -> PipelineToken {
        PipelineToken {
            session_id,
            capture_id: self.capture_id.fetch_add(1, Ordering::SeqCst) + 1,
        }
    }

    /// Claim the next translation slot.
    ///
    /// Kept apart from `capture_id` because the two answer different questions.
    /// A capture is superseded the moment a newer frame is taken, which is what
    /// `is_current` reports and what lets the loop drop a frame it no longer
    /// needs to OCR. A translation is not: once the capture loop stopped
    /// awaiting the model, frames keep being taken while a translation is in
    /// flight, and measuring that translation against `capture_id` would
    /// discard every result the moment it arrived.
    pub fn begin_translation(&self) -> u64 {
        self.translation_id.fetch_add(1, Ordering::SeqCst) + 1
    }

    /// Whether a translation's result is still the one the viewer should see.
    pub fn is_translation_current(&self, translation_id: u64) -> bool {
        self.translation_id.load(Ordering::SeqCst) == translation_id
    }

    pub fn invalidate_capture(&self) {
        self.capture_id.fetch_add(1, Ordering::SeqCst);
        // The region moved, so whatever is being translated came from somewhere
        // the viewer is no longer pointing at.
        self.translation_id.fetch_add(1, Ordering::SeqCst);
    }

    pub fn invalidate_session(&self) -> u64 {
        let invalidated_by = self.session_id.fetch_add(1, Ordering::SeqCst) + 1;
        self.capture_id.fetch_add(1, Ordering::SeqCst);
        self.translation_id.fetch_add(1, Ordering::SeqCst);
        invalidated_by
    }

    pub fn is_current(&self, token: PipelineToken) -> bool {
        self.session_id.load(Ordering::SeqCst) == token.session_id
            && self.capture_id.load(Ordering::SeqCst) == token.capture_id
    }

    pub fn is_session_current(&self, session_id: u64) -> bool {
        self.session_id.load(Ordering::SeqCst) == session_id
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn newer_session_invalidates_every_older_capture() {
        let clock = PipelineClock::default();
        let first_session = clock.begin_session();
        let first_capture = clock.next_capture(first_session);
        assert!(clock.is_current(first_capture));

        let second_session = clock.begin_session();

        assert!(!clock.is_current(first_capture));
        assert!(clock.is_session_current(second_session));
    }

    #[test]
    fn region_change_invalidates_in_flight_capture() {
        let clock = PipelineClock::default();
        let session = clock.begin_session();
        let capture = clock.next_capture(session);

        clock.invalidate_capture();

        assert!(!clock.is_current(capture));
    }

    // The defect this separation exists for: with translation moved off the
    // capture loop, frames keep arriving while the model works. Measured
    // against `capture_id`, every result would land stale and nothing would
    // ever reach the overlay.
    #[test]
    fn captures_taken_during_a_translation_do_not_supersede_it() {
        let clock = PipelineClock::default();
        let session = clock.begin_session();
        let translation = clock.begin_translation();

        for _ in 0..5 {
            clock.next_capture(session);
        }

        assert!(clock.is_translation_current(translation));
        assert!(clock.is_session_current(session));
    }

    #[test]
    fn a_newer_translation_supersedes_one_still_in_flight() {
        let clock = PipelineClock::default();
        clock.begin_session();
        let first = clock.begin_translation();
        let second = clock.begin_translation();

        assert!(!clock.is_translation_current(first));
        assert!(clock.is_translation_current(second));
    }

    #[test]
    fn moving_the_region_discards_a_translation_of_the_old_one() {
        let clock = PipelineClock::default();
        clock.begin_session();
        let translation = clock.begin_translation();

        clock.invalidate_capture();

        assert!(!clock.is_translation_current(translation));
    }

    #[test]
    fn a_new_session_discards_a_translation_from_the_previous_one() {
        let clock = PipelineClock::default();
        clock.begin_session();
        let translation = clock.begin_translation();

        clock.begin_session();

        assert!(!clock.is_translation_current(translation));
    }

    #[test]
    fn stop_invalidates_session_and_capture() {
        let clock = PipelineClock::default();
        let session = clock.begin_session();
        let capture = clock.next_capture(session);

        clock.invalidate_session();

        assert!(!clock.is_session_current(session));
        assert!(!clock.is_current(capture));
    }
}
