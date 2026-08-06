// =============================================================================
// BAND_WINDOW.RS - what one band remembers about itself
// =============================================================================
// `band_tracker` decides which lines belong to which band; this is what a band
// keeps once they do, and the arithmetic that turns it into the figures
// `band_verdict` judges.
//
// The window is bounded by *age*, not just by count. Over the measured session
// two bands were live video text for sixteen minutes and then became a frozen
// screen when playback stopped. A band is not one thing forever, so its
// evidence has to expire or it goes on being judged on what it used to be.
//
// Horizontal position is summarised as the *smallest* scatter among the left
// edge, the centre and the right edge. Centred subtitles hold their centre;
// left-aligned ones hold their left edge while the centre wanders with line
// length. Taking the minimum covers every alignment, and measurement confirmed
// it costs nothing: text belonging to the video scattered by 388-574 pixels on
// all three edges at once, because the camera moves them together.
// =============================================================================

use super::band_verdict::{is_same_cue, BandStats, Verdict};
use std::collections::VecDeque;

/// How long a band's evidence stays relevant.
pub(super) const WINDOW_MS: u64 = 90_000;

/// Frames a band already known to carry subtitles must read as something else
/// before it is actually demoted.
///
/// Without this the verdict crosses a threshold and back inside its own window,
/// and measurement put that at one frame in six on the bottom band and one in
/// three on the top - which is a subtitle that intermittently fails to appear,
/// worst on the shortest lines. Only demotion is delayed. Promotion is
/// immediate, because making a subtitle wait to be recognised is the very
/// failure this is meant to prevent.
const HOLD_THROUGH_FRAMES: usize = 8;

#[derive(Debug, Clone, Copy)]
struct Observation {
    at_ms: u64,
    left: f32,
    right: f32,
    chars: usize,
}

impl Observation {
    fn centre(&self) -> f32 {
        (self.left + self.right) / 2.0
    }

    fn width(&self) -> f32 {
        self.right - self.left
    }
}

/// How long a band that was carrying subtitles may stay demoted before its
/// evidence is thrown away and it is judged again from scratch.
///
/// A demotion feeds itself. `WINDOW_MS` is ninety seconds, and every noisy read
/// arriving while the band is held refreshes the observations that caused the
/// demotion - so one bad stretch can silence a subtitle track for as long as the
/// noise lasts. The measured session lost 71.7 seconds in one go and 30.5 in
/// another, with OCR reading text on 87 frames throughout. See issue #59.
///
/// Thirty seconds bounds that. The cost is paid by a band that really is
/// credits: it is re-admitted, gathers observations again, and is demoted once
/// more. Leaking a second or two of credits every thirty is a far better trade
/// than losing half a minute of dialogue, because the viewer can read credits
/// unaided and cannot recover dialogue that was never shown.
///
/// This bounds an unbroken run of `Churning`, which is the measured failure. A
/// band that alternates between `Churning` and another excluded verdict clears
/// the clock each time and is not covered; the rate thresholds either side of
/// the subtitle band make that alternation unlikely for a single band, and
/// clocking every exclusion would put `Static` - a frozen screen, correctly
/// judged and producing no new evidence - back on trial every thirty seconds.
pub(super) const MAX_DEMOTION_MS: u64 = 30_000;

/// Frames a re-admitted band is judged on before it may be demoted again.
///
/// Expiry leaves the window empty, and an empty window reads as `Glimpsed` -
/// which is *excluded*, so without this the band spends its first frames back
/// still dropped, then reaches `MIN_OBSERVATIONS` and is re-demoted on the same
/// eight noisy frames that demoted it before. Measured on the branch's own test
/// scenario, that returned the band for five frames in thirty seconds.
///
/// Holding it included for a full window instead means the second judgement is
/// made on as much evidence as the first, rather than on the first eight frames
/// after a reset.
pub(super) const READMITTED_GRACE_FRAMES: usize = 12;

/// One band's recent history.
#[derive(Debug)]
pub(super) struct TrackedBand {
    centre_y: f32,
    last_seen_ms: u64,
    window: VecDeque<Observation>,
    settled: Option<Verdict>,
    disagreeing: usize,
    /// When this band stopped being judged a subtitle, if it ever was one.
    demoted_since_ms: Option<u64>,
    /// Frames left of the reprieve granted by an expired demotion.
    grace_frames: usize,
}

impl TrackedBand {
    pub(super) fn new(centre_y: f32) -> Self {
        Self {
            centre_y,
            last_seen_ms: 0,
            window: VecDeque::new(),
            settled: None,
            disagreeing: 0,
            demoted_since_ms: None,
            grace_frames: 0,
        }
    }

    /// Throw away the evidence behind a demotion that has lasted too long.
    ///
    /// Called before this frame is recorded, so the band is judged on what
    /// arrives next rather than on the stretch that demoted it.
    ///
    /// An empty window reads as `Glimpsed`, not `Warming`, and `Glimpsed` is
    /// excluded - so clearing alone would hand the band back several frames
    /// later and then re-demote it on the first full window of noise. The grace
    /// frames are what actually re-admit it; see `READMITTED_GRACE_FRAMES`.
    pub(super) fn expire_a_stale_demotion(&mut self, at_ms: u64) {
        let overdue = self
            .demoted_since_ms
            .is_some_and(|since| at_ms.saturating_sub(since) >= MAX_DEMOTION_MS);
        if overdue {
            self.window.clear();
            self.settled = None;
            self.disagreeing = 0;
            self.demoted_since_ms = None;
            self.grace_frames = READMITTED_GRACE_FRAMES;
        }
    }

    /// Apply the freshly computed verdict, delaying only the demotion of a band
    /// already established as carrying subtitles.
    ///
    /// Everything else takes effect at once: a band that has never been a
    /// subtitle has nothing to protect, and holding a promotion back would cost
    /// exactly the cues this exists to keep.
    pub(super) fn settle(&mut self, raw: Verdict, at_ms: u64) -> Verdict {
        // A band just handed back its chance is included while it gathers the
        // evidence to be judged on. Without this it is dropped for the first
        // frames after the reset and then demoted again on a short window.
        if self.grace_frames > 0 {
            self.grace_frames -= 1;
            self.settled = Some(Verdict::Warming);
            self.demoted_since_ms = None;
            return Verdict::Warming;
        }
        let settled = match self.settled {
            Some(Verdict::Subtitle) if raw != Verdict::Subtitle => {
                self.disagreeing += 1;
                if self.disagreeing >= HOLD_THROUGH_FRAMES {
                    self.disagreeing = 0;
                    raw
                } else {
                    Verdict::Subtitle
                }
            }
            _ => {
                self.disagreeing = 0;
                raw
            }
        };
        // Only `Churning` is put on the clock, because only `Churning` feeds
        // itself: it is reached by the cue rate being *inflated*, and the noisy
        // reads that inflate it keep arriving and keep refreshing the window.
        //
        // `Static` is the opposite - a band judged static is one nothing is
        // happening in, so there is no stream of new evidence to re-litigate,
        // and expiring it would re-admit a frozen screen every thirty seconds
        // for nothing. `Scattered` is a property of where the text sits rather
        // than how fast it changes, and moves only when the text does.
        if settled == Verdict::Churning {
            self.demoted_since_ms.get_or_insert(at_ms);
        } else {
            self.demoted_since_ms = None;
        }
        self.settled = Some(settled);
        settled
    }

    pub(super) fn centre_y(&self) -> f32 {
        self.centre_y
    }

    /// Whether every observation has expired, leaving nothing to judge on.
    pub(super) fn is_stale(&self, at_ms: u64, retire_ms: u64) -> bool {
        at_ms.saturating_sub(self.last_seen_ms) > retire_ms
    }

    /// Add what one frame contributed and drop whatever has aged out.
    pub(super) fn record(
        &mut self,
        centre_y: f32,
        left: f32,
        right: f32,
        chars: usize,
        at_ms: u64,
    ) {
        // Drift with the band rather than pinning it where it was first seen:
        // subtitles shift by a few pixels between cues.
        self.centre_y = self.centre_y * 0.9 + centre_y * 0.1;
        self.last_seen_ms = at_ms;
        self.window.push_back(Observation {
            at_ms,
            left,
            right,
            chars,
        });
        while self
            .window
            .front()
            .is_some_and(|oldest| at_ms.saturating_sub(oldest.at_ms) > WINDOW_MS)
        {
            self.window.pop_front();
        }
    }

    pub(super) fn stats(&self, frame_interval_ms: u64) -> BandStats {
        let observations: Vec<&Observation> = self.window.iter().collect();
        let lefts: Vec<f32> = observations.iter().map(|o| o.left).collect();
        let centres: Vec<f32> = observations.iter().map(|o| o.centre()).collect();
        let rights: Vec<f32> = observations.iter().map(|o| o.right).collect();

        let mut cues = usize::from(!observations.is_empty());
        for pair in observations.windows(2) {
            if !is_same_cue(
                pair[1].width(),
                pair[1].chars,
                pair[0].width(),
                pair[0].chars,
            ) {
                cues += 1;
            }
        }

        BandStats {
            observations: observations.len(),
            centre_scatter: scatter(&lefts).min(scatter(&centres)).min(scatter(&rights)),
            cues,
            on_screen_ms: observations.len() as u64 * frame_interval_ms,
        }
    }
}

/// Standard deviation, or zero for fewer than two samples.
fn scatter(values: &[f32]) -> f32 {
    if values.len() < 2 {
        return 0.0;
    }
    let mean = values.iter().sum::<f32>() / values.len() as f32;
    (values.iter().map(|v| (v - mean).powi(2)).sum::<f32>() / values.len() as f32).sqrt()
}

#[cfg(test)]
#[path = "band_window_tests.rs"]
mod tests;
