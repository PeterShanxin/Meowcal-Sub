use super::*;

const DIALOGUE: [&str; 6] = [
    "I walked home. The trains had stopped.",
    "We should talk about this tomorrow.",
    "You never told me about the meeting.",
    "Where were you last night?",
    "I thought you had already left.",
    "There is something I need to tell you.",
];

fn read(tracker: &mut BandTracker, text: &str, at: u64) -> bool {
    !tracker
        .observe(
            &[text.to_owned()],
            &[LineBox {
                x: 400.0,
                y: 1000.0,
                width: 500.0,
                height: 40.0,
            }],
            at,
        )
        .included
        .is_empty()
}

#[test]
fn every_equal_width_cue_survives_two_minutes() {
    for interval in [125, 250, 500, 900] {
        for duration in [4_000, 8_000, 20_000] {
            let mut tracker = BandTracker::new(1832.0, interval);
            let count = 120_000 / duration;
            let mut seen = vec![false; count as usize];
            let mut delays = Vec::new();
            for at in (0..120_000u64).step_by(interval as usize) {
                let cue = (at / duration) as usize;
                if read(&mut tracker, DIALOGUE[cue % DIALOGUE.len()], at) && !seen[cue] {
                    seen[cue] = true;
                    delays.push(at - cue as u64 * duration);
                }
            }
            assert!(
                seen.iter().all(|kept| *kept),
                "interval={interval} duration={duration}: {seen:?}"
            );
            // Ignore initial acquisition; subsequent cues need at most one
            // confirming sample after the first sample sees the changed text.
            assert!(
                delays.iter().skip(1).all(|delay| *delay < 2 * interval),
                "{delays:?}"
            );
        }
    }
}

#[test]
fn a_new_cue_recovers_after_a_long_pause() {
    let mut tracker = BandTracker::new(1832.0, 250);
    for at in (0..120_000).step_by(250) {
        read(&mut tracker, DIALOGUE[0], at);
    }
    assert!(!read(&mut tracker, DIALOGUE[0], 120_000));
    read(&mut tracker, DIALOGUE[1], 120_250);
    assert!(read(&mut tracker, DIALOGUE[1], 120_500));
}

#[test]
fn short_numbers_negations_and_chinese_changes_are_new_cues() {
    let mut tracker = BandTracker::new(1832.0, 250);
    for (cue, text) in [
        "I can",
        "I can't",
        "Yes",
        "No",
        "Room 12",
        "Room 13",
        "我能去",
        "我不能去",
    ]
    .iter()
    .enumerate()
    {
        let mut kept = false;
        for offset in (0..8_000).step_by(250) {
            kept |= read(&mut tracker, text, cue as u64 * 8_000 + offset);
        }
        assert!(kept, "cue {cue} was lost");
    }
}

#[test]
fn static_text_and_transient_ocr_noise_do_not_periodically_recover() {
    let mut tracker = BandTracker::new(1832.0, 250);
    for at in (0..8_000).step_by(250) {
        read(&mut tracker, DIALOGUE[0], at);
    }
    let noise = [
        DIALOGUE[0],
        "I walked home. The trains had stapped.",
        DIALOGUE[0],
        "I walked home",
        DIALOGUE[0],
        "The trains had stopped.",
        DIALOGUE[0],
        "I walked home!  The trains had stopped...",
    ];
    for (index, at) in (8_000..120_000).step_by(250).enumerate() {
        assert!(
            !read(&mut tracker, noise[index % noise.len()], at),
            "static noise admitted at {at}"
        );
    }
}

#[test]
fn credits_and_timers_stay_held_then_stable_dialogue_recovers() {
    for period in [250, 500, 1_000, 1_500, 2_000] {
        let mut tracker = BandTracker::new(1832.0, 250);
        for at in (0..120_000).step_by(250) {
            let text = format!("Time remaining {} seconds", at / period);
            let kept = read(&mut tracker, &text, at);
            if at > 5_000 {
                assert!(!kept, "timer admitted at {at}, period {period}");
            }
        }
        let mut recovered = false;
        for at in (120_000..122_000).step_by(250) {
            recovered |= read(&mut tracker, DIALOGUE[0], at);
        }
        assert!(recovered, "a stable subtitle must recover from churning");
    }
}

#[test]
fn a_cue_can_reappear_after_a_blank_without_retiring_its_band() {
    let mut tracker = BandTracker::new(1832.0, 250);
    for at in (0..8_000).step_by(250) {
        read(&mut tracker, DIALOGUE[0], at);
    }
    tracker.observe(&[], &[], 8_000);
    read(&mut tracker, DIALOGUE[0], 8_250);
    assert!(read(&mut tracker, DIALOGUE[0], 8_500));
}

#[test]
fn uneven_sampling_does_not_turn_slow_dialogue_into_churn() {
    let mut tracker = BandTracker::new(1832.0, 250);
    for cue in 0..30 {
        let start = cue * 4_000;
        let mut kept = false;
        for offset in [0, 50, 600, 1_500, 2_200, 3_800] {
            kept |= read(
                &mut tracker,
                DIALOGUE[cue as usize % DIALOGUE.len()],
                start + offset,
            );
        }
        assert!(kept, "cue {cue}");
    }
}

#[test]
fn a_new_cue_recovers_even_when_ocr_alternates_one_wrong_glyph() {
    let mut tracker = BandTracker::new(1832.0, 250);
    for at in (0..8_000).step_by(250) {
        read(&mut tracker, DIALOGUE[0], at);
    }
    assert!(!read(&mut tracker, "Please wait by the entrance", 8_000));
    assert!(read(&mut tracker, "Please wait by the entranco", 8_250));
}

#[test]
fn an_initial_anchor_like_misread_cannot_pin_a_new_stable_cue() {
    let mut tracker = BandTracker::new(1832.0, 250);
    for at in (0..8_000).step_by(250) {
        read(&mut tracker, "The morning train is arriving", at);
    }
    read(&mut tracker, "The morning train is arrivinz", 8_000);
    read(&mut tracker, "The morning train is arriwinz", 8_250);
    assert!(read(&mut tracker, "The morning train is arriwinz", 8_500));
}

#[test]
fn consecutive_numeric_dialogue_is_admitted_at_normal_reading_speed() {
    let mut tracker = BandTracker::new(1832.0, 250);
    for (index, text) in ["Room 12", "Room 13", "Room 14"].iter().enumerate() {
        let mut kept = false;
        for offset in (0..4_000).step_by(250) {
            kept |= read(&mut tracker, text, index as u64 * 4_000 + offset);
        }
        assert!(kept, "{text}");
    }
}

#[test]
fn empty_ocr_frames_do_not_restart_an_established_counter() {
    let mut tracker = BandTracker::new(1832.0, 250);
    for at in (0..5_000).step_by(250) {
        read(&mut tracker, &format!("01:{:02}", at / 1_000), at);
    }
    for at in (5_000..7_500).step_by(250) {
        tracker.observe(&[], &[], at);
    }
    for at in (7_500..8_500).step_by(250) {
        assert!(!read(&mut tracker, "01:07", at));
    }
    // A genuinely held numeric reading can recover on observed stability.
    let mut recovered = false;
    for at in (8_500..11_500).step_by(250) {
        recovered |= read(&mut tracker, "01:08", at);
    }
    assert!(recovered);
    assert!(!read(&mut tracker, DIALOGUE[0], 11_500));
    assert!(read(&mut tracker, DIALOGUE[0], 11_750));
}

#[test]
fn an_ocr_outage_cannot_confirm_a_candidate_or_hide_fast_turnover() {
    let mut tracker = BandTracker::new(1832.0, 250);
    read(&mut tracker, DIALOGUE[0], 0);
    assert!(!read(&mut tracker, DIALOGUE[0], 10_000));
    assert!(read(&mut tracker, DIALOGUE[0], 10_250));
    read(&mut tracker, DIALOGUE[1], 20_000);
    assert!(!read(&mut tracker, DIALOGUE[1], 20_250));
    assert!(read(&mut tracker, DIALOGUE[1], 21_250));
}
