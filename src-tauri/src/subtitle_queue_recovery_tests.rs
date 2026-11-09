use super::tests::{admit, finish, observe, shown};
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

#[test]
fn a_paused_line_stays_visible_while_its_successor_is_translating() {
    let start = Instant::now();
    let mut queue = SubtitleQueue::new(Duration::from_millis(125));
    admit(&mut queue, "Wait here.", start, 1);
    let old = queue.next_job(start + Duration::from_millis(200)).unwrap();
    finish(&mut queue, &old, start + Duration::from_millis(500));
    shown(&mut queue, start + Duration::from_millis(500));
    observe(
        &mut queue,
        "Wait here.",
        start + Duration::from_secs(29),
        290,
    );
    admit(
        &mut queue,
        "Come inside.",
        start + Duration::from_secs(30),
        300,
    );
    assert!(queue
        .next_display(start + Duration::from_millis(30200))
        .is_none());
    let next = queue
        .next_job(start + Duration::from_millis(30200))
        .unwrap();
    finish(&mut queue, &next, start + Duration::from_millis(30800));
    assert_eq!(
        shown(&mut queue, start + Duration::from_millis(30800)).1,
        "Come inside."
    );
}

#[test]
fn one_late_ocr_glitch_cannot_retranslate_a_paused_line() {
    let start = Instant::now();
    let mut queue = SubtitleQueue::<String>::new(Duration::from_millis(125));
    admit(&mut queue, "Wait here.", start, 1);
    observe(
        &mut queue,
        "Wait here.",
        start + Duration::from_secs(30),
        300,
    );
    observe(
        &mut queue,
        "Wait hert.",
        start + Duration::from_secs(31),
        310,
    );
    observe(
        &mut queue,
        "Wait here.",
        start + Duration::from_millis(31125),
        311,
    );
    observe(
        &mut queue,
        "Wait here.",
        start + Duration::from_millis(31250),
        312,
    );
    assert_eq!(queue.stats(start).enqueued, 1);
}
