use super::*;

fn frame(text: &str, start: Instant, capture: u64) -> Frame {
    Frame {
        token: PipelineToken {
            session_id: 1,
            capture_id: capture,
        },
        region_generation: 0,
        text: text.to_string(),
        context_prompt: None,
        started: start,
        capture_ms: 5,
        ocr_ms: 40,
        sample_ms: 125,
    }
}

pub(super) fn observe(
    queue: &mut SubtitleQueue<String>,
    text: &str,
    now: Instant,
    capture: u64,
) -> Option<u64> {
    queue.observe(frame(text, now, capture), true, now)
}

pub(super) fn admit(
    queue: &mut SubtitleQueue<String>,
    text: &str,
    now: Instant,
    capture: u64,
) -> u64 {
    observe(queue, text, now, capture);
    observe(queue, text, now + Duration::from_millis(125), capture + 1)
        .expect("stable cue admitted")
}

pub(super) fn finish(queue: &mut SubtitleQueue<String>, job: &Job, now: Instant) {
    assert!(queue.complete(
        job,
        Completion {
            result: job.frame.text.clone(),
            text: "Go!".into(),
            presentation: Presentation::Translated,
            retryable: false
        },
        now
    ));
}

pub(super) fn shown(queue: &mut SubtitleQueue<String>, now: Instant) -> (u64, String) {
    match queue.next_display(now).expect("ready cue shown") {
        Display::Show { job, result, .. } => (job.cue_id, result),
        Display::Clear => panic!("cue must not be cleared"),
    }
}

#[test]
fn stable_negations_numbers_and_single_chinese_characters_are_separate_cues() {
    let start = Instant::now();
    let mut queue = SubtitleQueue::new(Duration::from_millis(125));
    let lines = [
        "I can",
        "I can't",
        "room 12",
        "room 13",
        "room 14",
        "我能去",
        "我不能去",
        "好",
        "不",
    ];
    for (i, line) in lines.iter().enumerate() {
        assert_eq!(
            admit(
                &mut queue,
                line,
                start + Duration::from_millis(i as u64 * 800),
                i as u64 * 10
            ),
            i as u64 + 1
        );
    }
    assert_eq!(queue.stats(start).enqueued, lines.len() as u64);
}

#[test]
fn busy_translation_preserves_every_stable_cue_in_fifo_order() {
    let start = Instant::now();
    let mut queue = SubtitleQueue::new(Duration::from_millis(125));
    admit(&mut queue, "First", start, 1);
    let a = queue.next_job(start + Duration::from_millis(200)).unwrap();
    admit(&mut queue, "Second", start + Duration::from_millis(800), 10);
    admit(
        &mut queue,
        "Third",
        start + Duration::from_millis(1_600),
        20,
    );
    assert!(
        queue.next_job(start + Duration::from_secs(2)).is_none(),
        "single inference slot"
    );
    finish(&mut queue, &a, start + Duration::from_secs(2));
    let b = queue.next_job(start + Duration::from_secs(2)).unwrap();
    finish(&mut queue, &b, start + Duration::from_millis(2_400));
    let c = queue
        .next_job(start + Duration::from_millis(2_400))
        .unwrap();
    finish(&mut queue, &c, start + Duration::from_millis(2_800));
    assert_eq!(
        shown(&mut queue, start + Duration::from_secs(3)),
        (1, "First".into())
    );
    assert!(queue
        .next_display(start + Duration::from_millis(3_349))
        .is_none());
    assert_eq!(
        shown(&mut queue, start + Duration::from_millis(3_350)),
        (2, "Second".into())
    );
    assert_eq!(
        shown(&mut queue, start + Duration::from_millis(3_700)),
        (3, "Third".into())
    );
    let stats = queue.stats(start + Duration::from_secs(5));
    assert_eq!((stats.depth, stats.displayed, stats.skipped), (0, 3, 0));
}

#[test]
fn a_paused_cue_outlives_the_queue_without_being_retranslated() {
    let start = Instant::now();
    let mut queue = SubtitleQueue::new(Duration::from_millis(125));
    admit(&mut queue, "Wait here.", start, 1);
    let job = queue.next_job(start + Duration::from_millis(200)).unwrap();
    finish(&mut queue, &job, start + Duration::from_millis(500));
    shown(&mut queue, start + Duration::from_millis(500));
    for i in 4..480 {
        let now = start + Duration::from_millis(i * 125);
        // A static band can be ineligible after six seconds. It still proves
        // that the source cue has not disappeared.
        assert!(queue
            .observe(frame("Wait here!", now, i), i < 48, now)
            .is_none());
        assert!(queue.next_display(now).is_none());
    }
    assert_eq!(
        (queue.stats(start).enqueued, queue.stats(start).depth),
        (1, 0)
    );
}

#[test]
fn transient_ocr_noise_and_one_blank_do_not_create_or_retire_a_cue() {
    let start = Instant::now();
    let mut queue = SubtitleQueue::new(Duration::from_millis(125));
    admit(&mut queue, "Please wait outside.", start, 1);
    for (i, text) in [
        "Please wait outsido",
        "Please wait outside!",
        "",
        "Please wait outside.",
        "Please wait",
        "Please wait outside.",
    ]
    .iter()
    .enumerate()
    {
        assert!(observe(
            &mut queue,
            text,
            start + Duration::from_millis(250 + i as u64 * 125),
            i as u64 + 3
        )
        .is_none());
    }
    assert_eq!(queue.stats(start).enqueued, 1);
}

#[test]
fn a_late_result_survives_empty_frames_until_its_reading_floor() {
    let start = Instant::now();
    let mut queue = SubtitleQueue::new(Duration::from_millis(125));
    admit(&mut queue, "Go!", start, 1);
    let job = queue.next_job(start + Duration::from_millis(200)).unwrap();
    for ms in [500, 625, 750] {
        observe(&mut queue, "", start + Duration::from_millis(ms), ms);
    }
    finish(&mut queue, &job, start + Duration::from_secs(2));
    shown(&mut queue, start + Duration::from_secs(2));
    assert!(queue
        .next_display(start + Duration::from_millis(2_349))
        .is_none());
    assert!(matches!(
        queue.next_display(start + Duration::from_millis(2_350)),
        Some(Display::Clear)
    ));
    assert!(queue.next_display(start + Duration::from_secs(3)).is_none());
    assert_eq!(
        admit(&mut queue, "Go!", start + Duration::from_secs(4), 40),
        2
    );
}

#[test]
fn moving_the_region_discards_queued_and_in_flight_work() {
    let start = Instant::now();
    let mut queue = SubtitleQueue::new(Duration::from_millis(125));
    admit(&mut queue, "Old region", start, 1);
    let old = queue.next_job(start + Duration::from_millis(200)).unwrap();
    queue.reset();
    assert!(!queue.complete(
        &old,
        Completion {
            result: "stale".into(),
            text: "stale".into(),
            presentation: Presentation::Translated,
            retryable: false
        },
        start
    ));
    assert_eq!(
        admit(&mut queue, "New region", start + Duration::from_secs(1), 10),
        2
    );
    let new = queue
        .next_job(start + Duration::from_millis(1_200))
        .unwrap();
    finish(&mut queue, &new, start + Duration::from_millis(1_500));
    assert_eq!(
        shown(&mut queue, start + Duration::from_millis(1_500)).1,
        "New region"
    );
}

#[test]
fn a_failed_current_cue_retries_after_cooldown_without_duplicating_pending_work() {
    let start = Instant::now();
    let mut queue = SubtitleQueue::new(Duration::from_millis(125));
    admit(&mut queue, "Retry", start, 1);
    let job = queue.next_job(start + Duration::from_millis(200)).unwrap();
    assert!(queue.complete(
        &job,
        Completion {
            result: "warming".into(),
            text: "".into(),
            presentation: Presentation::Preserve,
            retryable: true
        },
        start + Duration::from_millis(500)
    ));
    shown(&mut queue, start + Duration::from_millis(500));
    assert!(observe(
        &mut queue,
        "Retry",
        start + Duration::from_millis(2_999),
        30
    )
    .is_none());
    assert_eq!(
        observe(
            &mut queue,
            "Retry",
            start + Duration::from_millis(3_000),
            31
        ),
        Some(1)
    );
    for i in 32..80 {
        observe(
            &mut queue,
            "Retry",
            start + Duration::from_millis(i * 125),
            i,
        );
    }
    assert_eq!(
        (queue.stats(start).depth, queue.stats(start).enqueued),
        (1, 2)
    );
}

#[test]
fn an_explicit_source_only_result_clears_visible_state_and_keeps_active_identity() {
    let start = Instant::now();
    let mut queue = SubtitleQueue::new(Duration::from_millis(125));
    admit(&mut queue, "First", start, 1);
    let a = queue.next_job(start + Duration::from_millis(200)).unwrap();
    finish(&mut queue, &a, start + Duration::from_millis(500));
    shown(&mut queue, start + Duration::from_millis(500));
    admit(&mut queue, "Second", start + Duration::from_secs(1), 10);
    let b = queue
        .next_job(start + Duration::from_millis(1_200))
        .unwrap();
    queue.complete(
        &b,
        Completion {
            result: "source only".into(),
            text: String::new(),
            presentation: Presentation::Clear,
            retryable: false,
        },
        start + Duration::from_millis(1_300),
    );
    assert_eq!(
        shown(&mut queue, start + Duration::from_millis(1_300)).1,
        "source only"
    );
    assert!(!queue.is_busy());
    assert!(observe(&mut queue, "Second", start + Duration::from_secs(2), 20).is_none());
}

#[test]
fn slow_recognition_of_one_empty_frame_does_not_end_a_paused_cue() {
    let start = Instant::now();
    let mut queue = SubtitleQueue::new(Duration::from_millis(125));
    admit(&mut queue, "Wait here.", start, 1);
    let job = queue.next_job(start + Duration::from_millis(200)).unwrap();
    finish(&mut queue, &job, start + Duration::from_millis(500));
    shown(&mut queue, start + Duration::from_millis(500));
    queue.observe(
        frame("", start + Duration::from_secs(1), 10),
        false,
        start + Duration::from_secs(4),
    );
    assert!(observe(
        &mut queue,
        "Wait here.",
        start + Duration::from_millis(4_100),
        11
    )
    .is_none());
    assert!(observe(
        &mut queue,
        "Wait here.",
        start + Duration::from_millis(4_225),
        12
    )
    .is_none());
    assert_eq!(queue.stats(start).enqueued, 1);
}
