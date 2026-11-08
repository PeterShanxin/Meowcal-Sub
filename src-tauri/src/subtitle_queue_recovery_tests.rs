use super::tests::{admit, observe};
use super::*;

#[test]
fn ordinary_backlog_is_retained_and_only_severe_stalls_skip_with_a_count() {
    let start = Instant::now();
    let mut queue = SubtitleQueue::new(Duration::from_millis(125));
    for i in 0..10 {
        admit(
            &mut queue,
            &format!("Cue {i}"),
            start + Duration::from_millis(i * 800),
            i * 10,
        );
    }
    assert_eq!(
        (queue.stats(start).depth, queue.stats(start).skipped),
        (10, 0)
    );
    queue.recover_if_stalled(start + Duration::from_secs(16));
    assert_eq!(
        (queue.stats(start).depth, queue.stats(start).skipped),
        (1, 9)
    );
    let job = queue.next_job(start + Duration::from_secs(16)).unwrap();
    assert_eq!(job.frame.text, "Cue 9");
}

#[test]
fn the_depth_bound_recovers_to_the_current_cue_and_invalidates_old_completion() {
    let start = Instant::now();
    let mut queue = SubtitleQueue::new(Duration::from_millis(125));
    admit(&mut queue, "First", start, 1);
    let old = queue.next_job(start + Duration::from_millis(150)).unwrap();
    for i in 1..32 {
        admit(
            &mut queue,
            &format!("Cue {i}"),
            start + Duration::from_millis(i * 250),
            i * 2 + 1,
        );
    }
    assert_eq!(
        (queue.stats(start).depth, queue.stats(start).skipped),
        (1, 31)
    );
    assert!(!queue.complete(
        &old,
        Completion {
            result: "old".into(),
            text: "old".into(),
            presentation: Presentation::Translated,
            retryable: false
        },
        start
    ));
    assert_eq!(
        queue
            .next_job(start + Duration::from_secs(9))
            .unwrap()
            .frame
            .text,
        "Cue 31"
    );
}

#[test]
fn a_severely_expired_queue_cannot_restore_a_cue_that_is_no_longer_on_screen() {
    let start = Instant::now();
    let mut queue = SubtitleQueue::<String>::new(Duration::from_millis(125));
    admit(&mut queue, "Gone", start, 1);
    for ms in [500, 750, 1000] {
        observe(&mut queue, "", start + Duration::from_millis(ms), ms);
    }
    queue.recover_if_stalled(start + Duration::from_secs(16));
    assert!(queue.next_job(start + Duration::from_secs(16)).is_none());
    assert_eq!(
        (queue.stats(start).depth, queue.stats(start).skipped),
        (0, 1)
    );
}
