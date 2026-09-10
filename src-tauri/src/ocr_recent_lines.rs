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
/// entry per frame, against text no longer than the prompt carries.
const REMEMBERED_LINES: usize = 4;

/// How long a line stays worth comparing against.
///
/// A cue is on screen for two to four seconds. Six seconds covers a slow one
/// and its neighbour; past that, the same words are dialogue repeating rather
/// than one cue being re-read, and a viewer expects to see them again.
const WINDOW: Duration = Duration::from_secs(6);

/// The lines translated recently, newest last.
#[derive(Debug)]
pub struct RecentLines {
    entries: VecDeque<(String, Instant)>,
    /// How much of a read is compared: `prompt_max_source_chars`, applied to the
    /// cleaned source exactly as the prompt applies it. The model is given no
    /// more than this, so text past it cannot change the translation, and a long
    /// page would otherwise make every edit distance quadratic in the whole page.
    max_chars: usize,
}

impl RecentLines {
    pub fn new(max_chars: usize) -> Self {
        Self {
            entries: VecDeque::new(),
            max_chars,
        }
    }

    fn clip(&self, text: &str) -> String {
        crate::llm::clean_source_text(text)
            .chars()
            .take(self.max_chars)
            .collect()
    }

    /// How this read relates to the recent lines, judged against whichever it
    /// most resembles.
    ///
    /// `Repeat` if any remembered line says so; `Extended` only if that is the
    /// strongest verdict, so a read that extends one row while repeating another
    /// is still suppressed. Otherwise `New`.
    pub fn classify(&mut self, current: &str, now: Instant) -> LineChange {
        self.forget_stale(now);
        let current = self.clip(current);
        let current = current.as_str();

        let newest = self.entries.len().saturating_sub(1);
        let mut strongest = LineChange::New;
        let mut repeat_of = None;
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
                LineChange::Repeat => {
                    repeat_of = Some(position);
                    break;
                }
                LineChange::Extended => strongest = LineChange::Extended,
                LineChange::New => {}
            }
        }

        if let Some(position) = repeat_of {
            // Seeing the cue again means it is still on screen, so the window
            // has to measure time since it was last *seen* rather than since it
            // was translated. Clocking from the translation expired a cue that
            // had never left - a paused frame, a title card, a lyric - and the
            // next read of it came back `New` against an empty deque and earned
            // a second, differently worded translation over the one the viewer
            // was already reading.
            self.entries[position].1 = now;
            return LineChange::Repeat;
        }
        strongest
    }

    /// Remember a line that was sent for translation.
    pub fn remember(&mut self, line: &str, now: Instant) {
        self.forget_stale(now);
        let line = self.clip(line);
        self.entries.push_back((line, now));
        while self.entries.len() > REMEMBERED_LINES {
            self.entries.pop_front();
        }
    }

    /// Forget everything, for when the region empties and the cue is gone.
    pub fn clear(&mut self) {
        self.entries.clear();
    }

    /// Drop every entry older than the window, wherever it sits.
    ///
    /// Not just the front. `classify` refreshes whichever entry a repeat matched,
    /// and that entry is often not the newest - a title card re-read while
    /// dialogue plays under it, a cue whose other row arrived later. Refreshing
    /// in place leaves the deque out of timestamp order, so expiring only from
    /// the front let a fresh entry pin expired ones behind it: the dialogue they
    /// held came round again outside the window and was still suppressed as a
    /// repeat.
    ///
    /// Refreshing in place is what keeps the last entry the line most recently
    /// *translated*, which is the one `classify` compares in full. Moving a
    /// refreshed entry to the back instead would hand that comparison to a line
    /// that is merely still on screen.
    fn forget_stale(&mut self, now: Instant) {
        self.entries
            .retain(|(_, seen)| now.duration_since(*seen) <= WINDOW);
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
/// Being wrong here costs a suppressed subtitle: `resembles` asks whether a read
/// is a re-read of a remembered line, and a false positive drops a genuinely
/// new line. Genuinely unrelated English dialogue in the recorded session
/// reached 0.33 at p95, so this sits just above that - but "just above p95" also
/// means roughly one unrelated pair in twenty still clears it, which is why the
/// caller limits the comparison to the remembered window rather than the whole
/// session.
const SAME_CUE_FLOOR: f32 = 0.45;

#[cfg(test)]
#[path = "ocr_recent_lines_tests.rs"]
mod tests;
