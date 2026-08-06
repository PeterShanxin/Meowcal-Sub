// =============================================================================
// PIPELINE_REPEAT_POLICY.RS - what to do with a line we have already seen
// =============================================================================
// A re-read of the subtitle on screen is normally nothing: translating it again
// spends the one translation slot and puts a second rendering of the same line
// over the video.
//
// The exception is the passthrough. When the last translation came from the mock
// backend the viewer is looking at untranslated source text, so the line is
// worth another attempt - the engine may have recovered since. That retry needs
// a cooldown, or a stalled engine turns every frame into a failing call.
//
// Extracted from the capture loop because it is a decision with three outcomes
// and no I/O, and inline it could only be exercised by running the whole
// pipeline against a screen.
// =============================================================================

use std::time::Duration;

/// What to do with a read that repeats a line already translated.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RepeatAction {
    /// Nothing to do. The line on screen already says this.
    Skip(&'static str),
    /// The viewer is looking at untranslated text; try the engine again.
    RetryPassthrough,
}

/// How long to wait before retrying a line the passthrough handled.
pub const MOCK_RETRY_COOLDOWN: Duration = Duration::from_millis(2500);

/// Decide what a repeated read deserves.
///
/// `since_last_attempt` is measured from the last line sent for translation
/// rather than from the last failure, because a stalled engine produces no
/// failures to measure from - only passthroughs.
pub fn decide(last_was_passthrough: bool, since_last_attempt: Duration) -> RepeatAction {
    if !last_was_passthrough {
        return RepeatAction::Skip("duplicate_line");
    }
    if since_last_attempt < MOCK_RETRY_COOLDOWN {
        return RepeatAction::Skip("duplicate_mock_cooldown");
    }
    RepeatAction::RetryPassthrough
}

#[cfg(test)]
mod tests {
    use super::*;

    // The ordinary case: the line on screen is a real translation of this text.
    #[test]
    fn a_repeat_of_a_translated_line_is_skipped() {
        assert_eq!(
            decide(false, Duration::from_secs(60)),
            RepeatAction::Skip("duplicate_line")
        );
    }

    // The viewer is looking at untranslated source text, so it is worth asking
    // the engine again once the cooldown has passed.
    #[test]
    fn a_repeat_of_a_passthrough_is_retried_once_the_cooldown_passes() {
        assert_eq!(
            decide(true, MOCK_RETRY_COOLDOWN),
            RepeatAction::RetryPassthrough
        );
    }

    // Without the cooldown a stalled engine would turn every captured frame into
    // another failing call.
    #[test]
    fn a_passthrough_retry_waits_for_the_cooldown() {
        assert_eq!(
            decide(true, MOCK_RETRY_COOLDOWN - Duration::from_millis(1)),
            RepeatAction::Skip("duplicate_mock_cooldown")
        );
    }

    // The reasons reach the log, where they are what tells a stalled engine from
    // an ordinary quiet stretch.
    #[test]
    fn the_skip_reasons_are_distinct() {
        let RepeatAction::Skip(translated) = decide(false, Duration::ZERO) else {
            panic!("a translated repeat should be skipped");
        };
        let RepeatAction::Skip(cooling) = decide(true, Duration::ZERO) else {
            panic!("a passthrough inside the cooldown should be skipped");
        };
        assert_ne!(translated, cooling);
    }
}
