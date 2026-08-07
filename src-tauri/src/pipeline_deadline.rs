// =============================================================================
// PIPELINE_DEADLINE.RS - how long one line may hold the translation slot
// =============================================================================
// The pipeline runs one translation at a time, and while it does, the capture
// loop stays quiet - see `pipeline_translation::Translator::is_busy`. So a call
// that runs long does not merely arrive late: it blanks the overlay and
// swallows every subtitle that appears behind it. Issue #60 recorded a single
// call holding the slot for 135 seconds on a loaded machine.
//
// The race between "the engine answered", "the session stopped" and "we waited
// long enough" lives here rather than inline, because it is the part with three
// outcomes and no obvious ordering, and because a `select!` against an
// `AppHandle` cannot be tested without a running Tauri app.
// =============================================================================

use std::future::Future;
use std::time::Duration;
use tokio::sync::watch;

/// How long a line may hold the translation slot before it is abandoned.
///
/// Five seconds is chosen from measurement rather than taste. On a healthy
/// machine this cancels nothing: p99 across a recorded 34-minute session was
/// 2291ms. A subtitle cue is on screen for roughly two to four seconds, so a
/// translation that has not arrived by now has already lost the line it was
/// for, and showing it late would overwrite the *next* line's translation with
/// a stale one.
///
/// Abandoning is worth doing because it genuinely frees the engine: the runtime
/// runs with one server slot, and llama.cpp cancels a generation whose client
/// has disconnected, releasing the slot 9ms later - measured against the shipped
/// build, see `docs/evidence/2026-08-05-arm64-abandoned-request-frees-slot.json`.
/// That claim is load-bearing. Were it false, every abandoned line would leave
/// the engine saturated, the next line would miss its deadline too, and the
/// overlay's `engineSlow` hint would become permanent rather than occasional.
///
/// It must also stay above `manager::UNCONTEXTED_ATTEMPT_TIMEOUT_MS`, or the
/// manager's own retry and source-passthrough fallback can never run.
///
/// No thread count fixes this; see
/// `docs/evidence/2026-08-05-arm64-engine-thread-sweep.json`.
pub(crate) const TRANSLATION_DEADLINE: Duration = Duration::from_secs(5);

/// Room kept back from the deadline for the fallback after the engine gives up.
///
/// 400ms is enough only because the fallback is `MockBackend`, an in-process
/// copy of the source line with no I/O, and because that last attempt carries no
/// `timeout()` of its own - so it needs time to *return*, not time to work. A
/// fallback that did real work, such as a network backend added behind Foundry,
/// would need this raised and a cap of its own.
const FALLBACK_RESERVE_MS: u64 = 400;

/// How long the backends may spend on one line, whatever config asks for.
///
/// The manager's own budget comes from config and is measured in tens of
/// seconds; this deadline is five. Bounding one by the other is what makes the
/// fallback reachable rather than merely defined - without it the retry starts
/// and is killed mid-flight, which is the worst of both.
///
/// Every caller of the manager inherits this, including the `translate_once`
/// debug command, which is capped here rather than at its configured timeout.
/// That is intended: the cap describes what the engine can deliver, not what a
/// particular caller is willing to wait for.
pub(crate) fn backend_budget() -> Duration {
    TRANSLATION_DEADLINE.saturating_sub(Duration::from_millis(FALLBACK_RESERVE_MS))
}

/// What became of a line that claimed the translation slot.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum SlotOutcome<T> {
    /// The engine answered in time.
    Finished(T),
    /// The session stopped underneath us. Nothing should be shown.
    Cancelled,
    /// The engine did not answer in time and the line was abandoned.
    DeadlineMissed,
}

/// Await `work`, giving up when the deadline passes or the session stops.
///
/// Stopping is checked before the deadline on purpose: a session that has ended
/// wants silence, not a notice about engine speed.
///
/// Returning drops `work`, which is what disconnects the HTTP request and stops
/// the engine generating. That is the whole point - see `TRANSLATION_DEADLINE`.
pub(crate) async fn await_within_deadline<F>(
    work: F,
    deadline: Duration,
    stop_rx: &mut watch::Receiver<bool>,
) -> SlotOutcome<F::Output>
where
    F: Future,
{
    tokio::select! {
        biased;
        changed = stop_rx.changed() => {
            let _ = changed;
            SlotOutcome::Cancelled
        }
        output = work => SlotOutcome::Finished(output),
        _ = tokio::time::sleep(deadline) => SlotOutcome::DeadlineMissed,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn never_stops() -> watch::Receiver<bool> {
        let (tx, rx) = watch::channel(false);
        // Kept alive for the test's duration so `changed()` pends rather than
        // resolving with a closed-channel error and looking like a stop.
        Box::leak(Box::new(tx));
        rx
    }

    #[tokio::test]
    async fn a_translation_that_answers_in_time_is_returned() {
        let mut stop = never_stops();
        let outcome =
            await_within_deadline(async { "你好" }, Duration::from_secs(5), &mut stop).await;

        assert_eq!(outcome, SlotOutcome::Finished("你好"));
    }

    // The failure this module exists for. Issue #60 measured a single call
    // holding the slot for 135 seconds; the slot must come back at the deadline.
    #[tokio::test(start_paused = true)]
    async fn a_translation_that_runs_past_the_deadline_is_abandoned() {
        let mut stop = never_stops();
        let forever = async {
            tokio::time::sleep(Duration::from_secs(3_600)).await;
            "too late"
        };

        let outcome = await_within_deadline(forever, Duration::from_secs(5), &mut stop).await;

        assert_eq!(outcome, SlotOutcome::DeadlineMissed);
    }

    // A line that lands just inside the deadline is still wanted. Proves the
    // deadline is where the constant says it is rather than somewhere near it.
    #[tokio::test(start_paused = true)]
    async fn a_translation_that_lands_just_inside_the_deadline_still_counts() {
        let mut stop = never_stops();
        let nearly_late = async {
            tokio::time::sleep(Duration::from_millis(4_900)).await;
            "in time"
        };

        let outcome = await_within_deadline(nearly_late, Duration::from_secs(5), &mut stop).await;

        assert_eq!(outcome, SlotOutcome::Finished("in time"));
    }

    #[tokio::test(start_paused = true)]
    async fn stopping_the_session_cancels_a_translation_in_flight() {
        let (tx, mut rx) = watch::channel(false);
        let forever = async {
            tokio::time::sleep(Duration::from_secs(3_600)).await;
            "unwanted"
        };
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(100)).await;
            let _ = tx.send(true);
        });

        let outcome = await_within_deadline(forever, Duration::from_secs(5), &mut rx).await;

        assert_eq!(outcome, SlotOutcome::Cancelled);
    }

    // A stop that arrives in the same moment as the deadline must read as a
    // stop: the session is over, and an engine-speed notice would be noise.
    #[tokio::test(start_paused = true)]
    async fn a_stop_wins_over_a_deadline_that_falls_due_at_the_same_time() {
        let (tx, mut rx) = watch::channel(false);
        tx.send(true).expect("receiver is alive");
        let forever = async {
            tokio::time::sleep(Duration::from_secs(3_600)).await;
            "unwanted"
        };

        let outcome = await_within_deadline(forever, Duration::ZERO, &mut rx).await;

        assert_eq!(outcome, SlotOutcome::Cancelled);
    }
}
