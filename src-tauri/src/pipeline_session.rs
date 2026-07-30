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
}

impl PipelineClock {
    pub fn begin_session(&self) -> u64 {
        self.capture_id.store(0, Ordering::SeqCst);
        self.session_id.fetch_add(1, Ordering::SeqCst) + 1
    }

    pub fn next_capture(&self, session_id: u64) -> PipelineToken {
        PipelineToken {
            session_id,
            capture_id: self.capture_id.fetch_add(1, Ordering::SeqCst) + 1,
        }
    }

    pub fn invalidate_capture(&self) {
        self.capture_id.fetch_add(1, Ordering::SeqCst);
    }

    pub fn invalidate_session(&self) -> u64 {
        let invalidated_by = self.session_id.fetch_add(1, Ordering::SeqCst) + 1;
        self.capture_id.fetch_add(1, Ordering::SeqCst);
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
