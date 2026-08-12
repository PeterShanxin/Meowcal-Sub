//! Characterization tests for the overlay liveness state machine.
//!
//! These cover the recovery decision without a real WebView2: the state is
//! pure and clock-injected, so stale/recovered transitions are deterministic
//! and never sleep real seconds.

use super::*;

fn start() -> Instant {
    Instant::now()
}

fn later(moment: Instant, elapsed: Duration) -> Instant {
    moment + elapsed
}

/// A bootstrapped overlay whose last heartbeat is inside the threshold is
/// healthy: no recovery is needed.
#[test]
fn fresh_ready_and_heartbeat_are_healthy() {
    let t0 = start();
    let mut state = LivenessState::default();
    state.mark_ready(t0);
    state.mark_heartbeat(t0);

    assert!(!needs_recovery(
        &state,
        later(t0, Duration::from_secs(2)),
        HEARTBEAT_STALE_AFTER
    ));
}

/// A renderer that bootstrapped but never heartbeats is already suspect: the
/// wedge can sit between listener registration and the first beat.
#[test]
fn ready_without_any_heartbeat_is_stale() {
    let t0 = start();
    let mut state = LivenessState::default();
    state.mark_ready(t0);

    assert!(needs_recovery(
        &state,
        later(t0, Duration::from_millis(1)),
        HEARTBEAT_STALE_AFTER
    ));
}

/// An overlay that never bootstrapped cannot process events; the first show
/// must recover it rather than trusting a live native HWND.
#[test]
fn never_ready_is_stale() {
    let state = LivenessState::default();

    assert!(needs_recovery(&state, start(), HEARTBEAT_STALE_AFTER));
}

/// Staleness is strict: a heartbeat exactly at the threshold is not yet stale.
#[test]
fn heartbeat_at_the_threshold_is_not_stale() {
    let t0 = start();
    let mut state = LivenessState::default();
    state.mark_ready(t0);
    state.mark_heartbeat(t0);

    assert!(!needs_recovery(
        &state,
        later(t0, HEARTBEAT_STALE_AFTER),
        HEARTBEAT_STALE_AFTER
    ));
}

/// One beat past the threshold is stale.
#[test]
fn heartbeat_older_than_the_threshold_is_stale() {
    let t0 = start();
    let mut state = LivenessState::default();
    state.mark_ready(t0);
    state.mark_heartbeat(t0);

    assert!(needs_recovery(
        &state,
        later(t0, HEARTBEAT_STALE_AFTER + Duration::from_millis(1)),
        HEARTBEAT_STALE_AFTER
    ));
}

/// A fresh beat resets the window: a renderer that recovered keeps sending
/// heartbeats, so a new show sees it as healthy.
#[test]
fn a_fresh_heartbeat_resets_staleness() {
    let t0 = start();
    let mut state = LivenessState::default();
    state.mark_ready(t0);
    state.mark_heartbeat(t0);

    assert!(needs_recovery(
        &state,
        later(t0, Duration::from_secs(10)),
        HEARTBEAT_STALE_AFTER
    ));

    state.mark_heartbeat(later(t0, Duration::from_secs(10)));
    assert!(!needs_recovery(
        &state,
        later(t0, Duration::from_secs(11)),
        HEARTBEAT_STALE_AFTER
    ));
}

/// The recovery watermark: a ready event that predates the recovery start
/// must not count as "recovered after reload" - the reloaded page's ready is
/// the only one that proves the fresh document bootstrapped.
#[test]
fn recovery_success_requires_ready_after_the_recovery_began() {
    let t0 = start();
    let mut state = LivenessState::default();
    state.mark_ready(t0);
    state.begin_recovery(later(t0, Duration::from_secs(10)));

    assert!(!state.ready_since_recovery_start());

    state.mark_ready(later(t0, Duration::from_secs(11)));
    assert!(state.ready_since_recovery_start());

    state.finish_recovery();
    assert!(!state.ready_since_recovery_start());
}

/// Recovery is one-shot and bounded: after the wait, the recovery slot is
/// consumed even if readiness never arrived, so the caller cannot loop.
#[test]
fn recovery_finishes_without_ready_and_cannot_loop() {
    let t0 = start();
    let mut state = LivenessState::default();
    state.begin_recovery(t0);

    assert!(!state.ready_since_recovery_start());
    state.finish_recovery();
    assert!(!state.ready_since_recovery_start());

    // A stale renderer stays stale: the *next* show decides again.
    assert!(needs_recovery(
        &state,
        later(t0, Duration::from_secs(9)),
        HEARTBEAT_STALE_AFTER
    ));
}

/// Repeated normal start/stop cycles must not accumulate recovery or liveness
/// residue: after several cycles the overlay is exactly as healthy as after
/// the first.
#[test]
fn repeated_start_stop_cycles_do_not_accumulate_recovery_state() {
    let t0 = start();
    let mut state = LivenessState::default();

    for cycle in 0..3u64 {
        let at = later(t0, Duration::from_secs(cycle));
        state.mark_ready(at);
        state.mark_heartbeat(at);
        state.begin_recovery(at);
        state.mark_ready(at);
        state.finish_recovery();
    }

    let after = later(t0, Duration::from_secs(10));
    state.mark_heartbeat(after);
    assert!(!needs_recovery(&state, after, HEARTBEAT_STALE_AFTER));
}

/// mark_ready keeps the latest timestamp: a reloaded page overwrites the
/// original bootstrap time, which is what makes the watermark comparison
/// sound.
#[test]
fn mark_ready_keeps_the_latest_bootstrap() {
    let t0 = start();
    let mut state = LivenessState::default();
    state.mark_ready(t0);
    state.mark_ready(later(t0, Duration::from_secs(5)));

    assert_eq!(state.ready_at, Some(later(t0, Duration::from_secs(5))));
}
