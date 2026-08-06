// =============================================================================
// OCR_RECENT_LINES.RS - the last few lines translated, not just the last one
// =============================================================================
// `ocr_stability::classify` compares a read against one previous line, and a
// two-line subtitle defeats that outright: Windows OCR returns the top row on
// one frame and the bottom row on the next, so each read is compared against the
// *other* row and every one of them looks like fresh dialogue. In the session
// recorded in issue #59, 15 of 450 translations repeated a line already
// translated two to five steps earlier, inside a six-second window.
//
// Widening the comparison to the last few lines is what closes that. It is a
// window rather than a set: the same words returning a minute later are a real
// repeat in the dialogue and should be translated again.
// =============================================================================

use crate::ocr_stability::LineChange;
use std::collections::VecDeque;
use std::time::{Duration, Instant};

/// How many recent lines a new read is compared against.
///
/// A two-line cue alternates between two rows, and a cue change can leave one
/// row of the old cue on screen beside one of the new, so four covers the
/// observed alternations with room to spare. Longer costs an edit distance per
/// entry per frame, against text that is a subtitle's length.
const REMEMBERED_LINES: usize = 4;

/// How long a line stays worth comparing against.
///
/// A cue is on screen for two to four seconds. Six seconds covers a slow one
/// and its neighbour; past that, the same words are dialogue repeating rather
/// than one cue being re-read, and a viewer expects to see them again.
const WINDOW: Duration = Duration::from_secs(6);

/// The lines translated recently, newest last.
#[derive(Debug, Default)]
pub struct RecentLines {
    entries: VecDeque<(String, Instant)>,
}

impl RecentLines {
    pub fn new() -> Self {
        Self::default()
    }

    /// How this read relates to the recent lines, judged against whichever it
    /// most resembles.
    ///
    /// `Repeat` if any remembered line says so; `Extended` only if that is the
    /// strongest verdict, so a read that extends one row while repeating another
    /// is still suppressed. Otherwise `New`.
    pub fn classify(&mut self, current: &str, now: Instant) -> LineChange {
        self.forget_stale(now);

        let newest = self.entries.len().saturating_sub(1);
        let mut strongest = LineChange::New;
        for (position, (line, _)) in self.entries.iter().enumerate() {
            // The full comparison only against the line actually on screen.
            // `ocr_stability` treats a read contained inside the previous one as
            // a re-read that lost characters, which is right for the line being
            // re-read and wrong for the three behind it: normalising strips
            // spaces and punctuation, so `No.` is contained in `I don't know.`
            // and a two-word reply vanished whenever any remembered line
            // happened to spell it. Older lines have to actually resemble the
            // read to count.
            let verdict = if position == newest {
                crate::ocr_stability::classify(line, current)
            } else if resembles(line, current) {
                LineChange::Repeat
            } else {
                LineChange::New
            };
            match verdict {
                LineChange::Repeat => return LineChange::Repeat,
                LineChange::Extended => strongest = LineChange::Extended,
                LineChange::New => {}
            }
        }
        strongest
    }

    /// Whether this read is a worse rendering of something already on screen.
    ///
    /// Asked only of reads that are about to be translated. The comparison is
    /// against the remembered line this read most resembles - the one it would
    /// replace - because a cue is re-read many times and only some of those
    /// reads are legible. See `ocr_corruption`.
    pub fn is_worse_than_a_recent_read(&self, current: &str) -> bool {
        self.most_similar(current)
            .is_some_and(|line| crate::ocr_corruption::is_worse_read(current, line))
    }

    /// The remembered line sharing the most characters with this read.
    ///
    /// Only lines that share more than nothing are eligible: with no floor, an
    /// unrelated line would be returned as "most similar" merely by being the
    /// only candidate, and a worse-read comparison against unrelated dialogue
    /// would suppress real subtitles.
    fn most_similar(&self, current: &str) -> Option<&str> {
        let current_chars = crate::ocr_stability::normalize(current);
        self.entries
            .iter()
            .map(|(line, _)| {
                let score = crate::ocr_stability::similarity(
                    &crate::ocr_stability::normalize(line),
                    &current_chars,
                );
                (line.as_str(), score)
            })
            .filter(|(_, score)| *score >= SAME_CUE_FLOOR)
            .max_by(|(_, left), (_, right)| left.total_cmp(right))
            .map(|(line, _)| line)
    }

    /// Remember a line that was sent for translation.
    pub fn remember(&mut self, line: &str, now: Instant) {
        self.forget_stale(now);
        self.entries.push_back((line.to_string(), now));
        while self.entries.len() > REMEMBERED_LINES {
            self.entries.pop_front();
        }
    }

    /// Forget everything, for when the region empties and the cue is gone.
    pub fn clear(&mut self) {
        self.entries.clear();
    }

    fn forget_stale(&mut self, now: Instant) {
        while let Some((_, seen)) = self.entries.front() {
            if now.duration_since(*seen) > WINDOW {
                self.entries.pop_front();
            } else {
                break;
            }
        }
    }
}

/// Whether two reads share enough to be renderings of one cue.
///
/// Used for the lines *behind* the one on screen, where the containment rule
/// does not apply. Deliberately the same threshold `ocr_stability` uses to call
/// two reads the same line, because that is the claim being made.
fn resembles(line: &str, current: &str) -> bool {
    let remembered = crate::ocr_stability::normalize(line);
    let read = crate::ocr_stability::normalize(current);
    crate::ocr_stability::similarity(&remembered, &read) >= SAME_CUE_FLOOR
}

/// How much two reads must share before one is treated as a rendering of the
/// other.
///
/// Being wrong here costs a suppressed subtitle: `is_worse_than_a_recent_read`
/// pairs a read against the remembered line it most resembles, and its caller
/// drops the frame. Genuinely unrelated English dialogue in the recorded session
/// reached 0.33 at p95, so this sits just above that - but "just above p95" also
/// means roughly one unrelated pair in twenty still clears it, which is why the
/// caller asks only about reads already judged to be the same cue.
const SAME_CUE_FLOOR: f32 = 0.45;

#[cfg(test)]
#[path = "ocr_recent_lines_tests.rs"]
mod tests;
