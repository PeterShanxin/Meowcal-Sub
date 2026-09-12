//! Raw spatial and turnover evidence for diagnostics. `TrackedBand::settle`
//! combines it with confirmed text identity before admitting a reading. Slow
//! turnover alone cannot veto a new cue. Vertical position is not a criterion.
/// Largest centre-line wander, as a fraction of the capture region's width,
/// that still counts as holding position.
///
/// Measured subtitles scattered by 5-9% of the region width and scene text by
/// 21-31%. The threshold sits in the gap with roughly a factor of two of margin
/// on each side, rather than being fitted to either group.
const MAX_CENTRE_SCATTER: f32 = 0.15;

/// Fewest distinct readings per minute on screen that still counts as changing.
///
/// Measured subtitles produced 37-44 and a frozen screen 2.2-2.5, so this is
/// nearly four times the static rate and well under a quarter of the subtitle
/// rate. Expressed per minute *on screen* rather than per minute of session, so
/// a band that appears rarely - which is what the top band does - is judged on
/// how it behaves while it is up, not on how often it is up.
pub(super) const MIN_CUE_RATE_PER_MINUTE: f32 = 10.0;

/// Most distinct readings per minute on screen that still counts as a subtitle.
///
/// A second recorded session ran into end credits: ten evenly spaced bands
/// spanning the whole frame, holding position well enough to pass the scatter
/// test - unlike the first session's scene text, which the camera swept around -
/// and so translated in full. Text that turns over four times faster than
/// dialogue is not dialogue. Measured subtitles reached 44 and the slowest thing
/// that was not a subtitle 67, in either session; this sits between them.
pub(super) const MAX_CUE_RATE_PER_MINUTE: f32 = 55.0;

/// Observations a band needs before it is worth translating at all.
///
/// Two sightings in a ninety-second window is not "not judged yet", it is a
/// glimpse - and measurement shows that is what stray recognitions off the
/// video look like, since they never recur often enough to be judged and so
/// would be translated forever under a default-open rule. A real cue lasts
/// about five frames, so a band that matters clears this within its first cue.
const MIN_OBSERVATIONS_TO_TRUST: usize = 3;

/// Observations a band needs before it can be judged at all.
///
/// Small on purpose. The busy bottom band reaches this in about three seconds
/// and the sparse top band in about forty, and until then the band is included
/// rather than excluded - see `Verdict::Warming`.
const MIN_OBSERVATIONS: usize = 8;

/// What a band's recent history says about it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    /// Not enough history to judge. Included anyway: dropping a band because
    /// nothing is known about it yet would silently lose the first cues of a
    /// session, and the top band is exactly the one that takes longest to
    /// gather evidence.
    Warming,
    /// Seen once or twice and then gone - a stray recognition off the video
    /// rather than a band that exists. Excluded, because unlike `Warming` this
    /// is not an absence of evidence but evidence of absence: a real cue lasts
    /// several frames, and these never recur.
    Glimpsed,
    /// Holds its position and changes at a subtitle's pace.
    Subtitle,
    /// Holds its position but hardly ever changes - a watermark, a paused
    /// player, a frozen screen.
    Static,
    /// Changes constantly and will not hold still horizontally - text that
    /// belongs to the video rather than to the subtitle track.
    Scattered,
    /// Holds its position but turns over far faster than dialogue - credits
    /// rolling past, a list scrolling, a timer counting.
    Churning,
}

impl Verdict {
    /// Whether lines in this band should be translated.
    ///
    /// Exclusion requires evidence; everything else is included.
    pub fn is_included(self) -> bool {
        matches!(self, Verdict::Warming | Verdict::Subtitle)
    }

    /// Whether this verdict rests on enough history to be worth reporting.
    ///
    /// A glimpse is held back every few seconds all session long, so logging
    /// each one would bury the drops that actually explain a missing subtitle.
    pub fn is_worth_reporting(self) -> bool {
        !matches!(self, Verdict::Glimpsed)
    }

    /// Why a band was excluded, for the log line that reports the drop.
    ///
    /// `ocr_gate` established that text is never discarded silently, and a band
    /// disappearing from a translation is exactly the kind of thing that is
    /// impossible to diagnose after the fact without this.
    pub fn reason(self) -> Option<&'static str> {
        match self {
            Verdict::Static => Some("unchanging"),
            Verdict::Scattered => Some("position unstable"),
            Verdict::Churning => Some("changing too fast for dialogue"),
            Verdict::Glimpsed => Some("seen too briefly to be a band"),
            Verdict::Warming | Verdict::Subtitle => None,
        }
    }
}

/// What a band's window of observations amounts to.
///
/// Kept as a plain struct so the arithmetic that produces it and the thresholds
/// that judge it can be tested apart from each other.
#[derive(Debug, Clone, Copy)]
pub struct BandStats {
    /// How many readings the window holds.
    pub observations: usize,
    /// Standard deviation of the line centre, in captured pixels.
    pub centre_scatter: f32,
    /// Distinct readings in the window, counted with a tolerance that survives
    /// a misread glyph.
    pub cues: usize,
    /// Milliseconds of screen time the window covers.
    pub on_screen_ms: u64,
}

impl BandStats {
    /// Distinct readings per minute of screen time.
    pub fn cue_rate_per_minute(&self) -> f32 {
        if self.on_screen_ms == 0 {
            return 0.0;
        }
        self.cues as f32 / self.on_screen_ms as f32 * 60_000.0
    }
}

/// Judge a band from its window.
///
/// `region_width` scales the scatter threshold so the same rule holds whatever
/// resolution the frame arrived at - the pipeline normalises high-DPI captures
/// and scales oversized ones, so an absolute pixel threshold would mean
/// different things on different displays.
pub fn classify(stats: &BandStats, region_width: f32) -> Verdict {
    if stats.observations < MIN_OBSERVATIONS_TO_TRUST {
        return Verdict::Glimpsed;
    }
    if stats.observations < MIN_OBSERVATIONS {
        return Verdict::Warming;
    }
    if stats.centre_scatter > MAX_CENTRE_SCATTER * region_width {
        return Verdict::Scattered;
    }
    let rate = stats.cue_rate_per_minute();
    if rate < MIN_CUE_RATE_PER_MINUTE {
        return Verdict::Static;
    }
    if rate > MAX_CUE_RATE_PER_MINUTE {
        return Verdict::Churning;
    }
    Verdict::Subtitle
}

#[cfg(test)]
mod tests {
    use super::*;

    const REGION: f32 = 1832.0;

    fn stats(
        observations: usize,
        centre_scatter: f32,
        cues: usize,
        on_screen_ms: u64,
    ) -> BandStats {
        BandStats {
            observations,
            centre_scatter,
            cues,
            on_screen_ms,
        }
    }

    // The four bands the measured session actually contained, at the numbers it
    // actually produced. These are the cases the feature exists to get right.
    #[test]
    fn the_measured_subtitle_bands_are_recognised() {
        // Bottom pair: 344 cues over 8.2 minutes, 567 over 13.7.
        assert_eq!(
            classify(&stats(1414, 134.0, 344, 473_000), REGION),
            Verdict::Subtitle
        );
        assert_eq!(
            classify(&stats(2358, 110.0, 567, 789_000), REGION),
            Verdict::Subtitle
        );
        // Top pair, present for a fraction of the session and still recognised
        // - the whole point of rating a band on its time on screen.
        assert_eq!(
            classify(&stats(182, 96.0, 42, 47_000), REGION),
            Verdict::Subtitle
        );
        assert_eq!(
            classify(&stats(88, 172.0, 19, 23_000), REGION),
            Verdict::Subtitle
        );
    }

    // Playback stopped halfway through the measured session and a static screen
    // sat there for sixteen minutes. Both bands it produced held position
    // perfectly, so only the cue rate rejects them.
    #[test]
    fn a_frozen_screen_is_not_a_subtitle() {
        assert_eq!(
            classify(&stats(2778, 56.0, 38, 912_000), REGION),
            Verdict::Static
        );
        assert_eq!(
            classify(&stats(2811, 115.0, 35, 953_000), REGION),
            Verdict::Static
        );
    }

    // Text belonging to the video - the newspaper in shot. It changes far faster
    // than a subtitle, so only the scatter rejects it.
    #[test]
    fn text_in_the_video_is_not_a_subtitle() {
        for scatter in [389.0, 431.0, 502.0, 575.0] {
            assert_eq!(
                classify(&stats(120, scatter, 200, 60_000), REGION),
                Verdict::Scattered,
                "scatter {scatter}"
            );
        }
    }

    #[test]
    fn a_band_with_too_little_history_is_included_rather_than_dropped() {
        let verdict = classify(&stats(MIN_OBSERVATIONS - 1, 9_000.0, 0, 1), REGION);
        assert_eq!(verdict, Verdict::Warming);
        assert!(verdict.is_included(), "warming bands must still be read");
    }

    // The second session ran into end credits: ten evenly spaced bands that
    // held position well enough to pass the scatter test and would otherwise
    // have been translated in full.
    #[test]
    fn end_credits_are_not_subtitles() {
        // 540 frames at 134ms is the block the second session actually held,
        // and 140-224 cues a minute is the rate it actually turned over at.
        for cues in [126, 202] {
            assert_eq!(
                classify(&stats(540, 200.0, cues, 72_000), 1857.0),
                Verdict::Churning,
                "{cues} cues"
            );
        }
    }

    // The gap the ceiling sits in, from both recorded sessions: nothing that
    // was a subtitle exceeded 44 a minute, and nothing that was not fell below
    // 67. Both sides are checked so a later tweak cannot quietly close it.
    #[test]
    fn the_rate_ceiling_sits_between_what_was_measured() {
        let per_minute = |rate: usize| stats(600, 100.0, rate, 60_000);
        assert_eq!(classify(&per_minute(44), 1832.0), Verdict::Subtitle);
        assert_eq!(classify(&per_minute(67), 1832.0), Verdict::Churning);
    }

    #[test]
    fn only_the_excluded_verdicts_carry_a_reason() {
        assert!(Verdict::Subtitle.is_included());
        assert!(!Verdict::Static.is_included());
        assert!(!Verdict::Scattered.is_included());
        assert_eq!(Verdict::Subtitle.reason(), None);
        assert_eq!(Verdict::Warming.reason(), None);
        assert!(!Verdict::Churning.is_included());
        assert!(!Verdict::Glimpsed.is_included());
        assert!(!Verdict::Glimpsed.is_worth_reporting());
        assert!(Verdict::Static.is_worth_reporting());
        assert!(Verdict::Static.reason().is_some());
        assert!(Verdict::Scattered.reason().is_some());
        assert!(Verdict::Churning.reason().is_some());
    }

    // The scatter threshold is a fraction of the region, so a narrower capture
    // must judge the same wander more harshly.
    #[test]
    fn the_scatter_threshold_follows_the_region_width() {
        let wander = stats(100, 200.0, 40, 60_000);
        assert_eq!(classify(&wander, 1832.0), Verdict::Subtitle);
        assert_eq!(classify(&wander, 800.0), Verdict::Scattered);
    }

    #[test]
    fn a_window_covering_no_screen_time_cannot_be_called_a_subtitle() {
        // Division by zero would otherwise produce an infinite rate and admit
        // anything at all.
        assert_eq!(classify(&stats(20, 10.0, 5, 0), REGION), Verdict::Static);
    }
}
