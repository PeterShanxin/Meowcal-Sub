use crate::ocr_recent_lines::*;
use crate::ocr_stability::LineChange;
use std::time::{Duration, Instant};

fn seen(lines: &[&str], now: Instant) -> RecentLines {
    let mut recent = RecentLines::new();
    for line in lines {
        recent.remember(line, now);
    }
    recent
}

// The failure this module exists for. OCR returns the top row on one frame and
// the bottom row on the next, so a single-slot memory compares each read against
// the other row and calls both of them fresh dialogue.
#[test]
fn a_two_line_cue_read_row_by_row_is_not_translated_twice() {
    let now = Instant::now();
    let top = "However, isn't he a hero from an era";
    let bottom = "where the Mystics have relatively thinned out?";

    let mut recent = seen(&[top, bottom], now);

    assert_eq!(recent.classify(top, now), LineChange::Repeat);
    assert_eq!(recent.classify(bottom, now), LineChange::Repeat);
}

// The window has to end. The same words a minute later are the dialogue
// repeating, and a viewer expects to see them translated again.
#[test]
fn a_line_returning_long_afterwards_is_fresh_dialogue() {
    let now = Instant::now();
    let line = "Where are you going?";
    let mut recent = seen(&[line], now);

    let much_later = now + Duration::from_secs(30);
    assert_eq!(recent.classify(line, much_later), LineChange::New);
}

// Genuinely new dialogue must still get through, or the overlay goes quiet.
#[test]
fn different_dialogue_is_still_translated() {
    let now = Instant::now();
    let mut recent = seen(
        &[
            "However, isn't he a hero from an era",
            "where the Mystics have relatively thinned out?",
        ],
        now,
    );

    assert_eq!(
        recent.classify("Lionheart was called the Wandering King", now),
        LineChange::New
    );
}

// Only the last few lines are held, so a cue from further back does not suppress
// dialogue that has legitimately come round again.
#[test]
fn only_the_most_recent_lines_are_remembered() {
    let now = Instant::now();
    let first = "Where are you going?";
    let mut recent = seen(
        &[
            first,
            "I told you to wait outside",
            "Nobody has seen him since winter",
            "Bring the horses round the back",
            "We leave before dawn",
        ],
        now,
    );

    assert_eq!(recent.classify(first, now), LineChange::New);
}

// A read that extends one remembered row while repeating another is a re-read,
// not new text - taking `Extended` there would translate the cue a second time.
#[test]
fn repeating_one_row_outweighs_extending_another() {
    let now = Instant::now();
    let mut recent = seen(&["Mystics have", "However, isn't he a hero"], now);

    assert_eq!(
        recent.classify("However, isn't he a hero", now),
        LineChange::Repeat
    );
}

// The second row of a two-line cue arrives carrying OCR noise of its own. It has
// to be translated anyway: it is the only read that will ever contain that row,
// and a read the pipeline declines to translate is never remembered, so
// declining once declines for the life of the cue.
//
// Both strings are taken from this branch's own `ocr_gate` fixtures, and the
// gate passes the second - one garbled token in five is under its threshold. A
// later filter refusing what the gate deliberately admitted is the bug.
#[test]
fn a_second_row_arriving_with_noise_is_still_translated() {
    let now = Instant::now();
    let mut recent = seen(&["Where the Mystics have"], now);

    assert_eq!(
        recent.classify("Where the Mystics have relatively;thinnedput?", now),
        LineChange::Extended,
        "the row that just appeared is new text, not a competing rendering"
    );
}

// A two-word reply is contained inside longer remembered lines once spacing and
// punctuation are stripped - `No.` is inside `I don't know.` - so comparing a
// read against four remembered lines instead of one made short exchanges vanish.
#[test]
fn a_short_reply_is_not_swallowed_by_the_lines_behind_it() {
    let now = Instant::now();
    let mut recent = seen(
        &[
            "However, isn't he a hero from an era",
            "where the Mystics have relatively thinned out?",
            "I told you to wait outside",
            "I don't know.",
        ],
        now,
    );

    // The newest line is still compared in full, so a re-read of *it* is caught.
    assert_eq!(recent.classify("I don't know", now), LineChange::Repeat);
    // But a reply that merely spells a substring of an older one is dialogue.
    assert_eq!(recent.classify("No.", now), LineChange::New);
    assert_eq!(recent.classify("Wait.", now), LineChange::New);
    assert_eq!(recent.classify("Are you?", now), LineChange::New);
}

// An empty memory is the state at session start and after the region empties.
#[test]
fn nothing_remembered_means_every_read_is_new() {
    let now = Instant::now();
    let mut recent = RecentLines::new();

    assert_eq!(
        recent.classify("Where are you going?", now),
        LineChange::New
    );
}

// A cue that stays on screen longer than the window - a paused frame, a title
// card, a lyric - must not age out from under itself. Clocking the window from
// the translation rather than from the last sighting expired a line that had
// never left, and the next read came back `New` against an empty memory and
// earned a second, differently worded translation over the one being read.
#[test]
fn a_cue_still_on_screen_does_not_expire_and_get_translated_twice() {
    let start = Instant::now();
    let line = "Where the Mystics have relatively thinned out?";
    let mut recent = seen(&[line], start);

    // Re-read every 250ms for well past the six-second window.
    let mut at = start;
    for _ in 0..40 {
        at += Duration::from_millis(250);
        assert_eq!(
            recent.classify(line, at),
            LineChange::Repeat,
            "still the same cue at {:?}",
            at.duration_since(start)
        );
    }
}

// The subtitle leaving the region has to reset the memory, or the identical cue
// returning would be suppressed as a re-read of a line no longer on screen.
#[test]
fn clearing_lets_the_same_line_be_translated_again() {
    let now = Instant::now();
    let line = "Where are you going?";
    let mut recent = seen(&[line], now);
    assert_eq!(recent.classify(line, now), LineChange::Repeat);

    recent.clear();

    assert_eq!(recent.classify(line, now), LineChange::New);
}
