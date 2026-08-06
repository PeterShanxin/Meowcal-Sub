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

// The point of the scoring: a mangled re-read must not replace the clean line
// already on screen.
#[test]
fn a_mangled_re_read_is_recognised_as_worse_than_what_is_displayed() {
    let now = Instant::now();
    let recent = seen(&["where the Mystics have relatively thinned out?"], now);

    assert!(recent.is_worse_than_a_recent_read("Wh€reythe MFtiÄhave relatively;thinnedput?"));
}

// And a cleaner later read must still be allowed to replace a mangled one.
#[test]
fn a_cleaner_later_read_is_not_judged_worse() {
    let now = Instant::now();
    let recent = seen(&["bf//dzz:: However, isn't he a hero from an era"], now);

    assert!(!recent.is_worse_than_a_recent_read("However, isn't he a hero from an era"));
}

// Unrelated dialogue must never be paired off against a remembered line: doing
// so would let an ordinary new subtitle be suppressed for scoring worse than
// something it has nothing to do with.
//
// The last pair is the one that matters. It sits in the band that a lower floor
// admitted - similar enough to be picked as "most similar", different enough
// that `ocr_stability` calls it fresh dialogue - and it is shorter, so it lost
// the length tie-break and vanished.
#[test]
fn unrelated_dialogue_is_not_compared_against_a_remembered_line() {
    let now = Instant::now();
    let recent = seen(&["where the Mystics have relatively thinned out?"], now);
    assert!(!recent.is_worse_than_a_recent_read("Hey"));
    assert!(!recent.is_worse_than_a_recent_read("Lionheart"));

    let exchange = seen(&["You should have told me before."], now);
    assert!(
        !exchange.is_worse_than_a_recent_read("You should go."),
        "a shorter, loosely similar line is fresh dialogue, not a worse re-read"
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
fn nothing_remembered_means_nothing_to_be_worse_than() {
    let now = Instant::now();
    let mut recent = RecentLines::new();

    assert_eq!(
        recent.classify("Where are you going?", now),
        LineChange::New
    );
    assert!(!recent.is_worse_than_a_recent_read("Where are you going?"));
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
