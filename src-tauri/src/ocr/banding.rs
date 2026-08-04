// =============================================================================
// BANDING.RS - what band selection hands back for one frame
// =============================================================================
// The result types, kept apart from the machinery that produces them, because
// these are what the rest of the app sees and they outlive any particular way
// of deciding them.
//
// The shape matters: bands stay separate rather than being flattened into one
// list of lines, so that a caller can put each band's translation where its
// text was. Nothing does that yet - the pipeline joins them - but throwing the
// grouping away here would make it impossible later. See issue #57.
// =============================================================================

use super::band_verdict::Verdict;

/// Lines that share a band, with where to put their translation.
#[derive(Debug, Clone, PartialEq)]
pub struct BandGroup {
    /// Vertical centre of the band in the recognition frame's pixels.
    ///
    /// `f32::NAN` for the group holding lines whose geometry Windows would not
    /// report. They are still translated - losing a subtitle because its
    /// position is unknown is worse than not knowing the position - but they
    /// have no place to be put, so a caller must fall back to its default.
    pub centre_y: f32,
    /// Indices into the `lines` slice that was observed, in the order given.
    pub lines: Vec<usize>,
}

/// A band whose lines were held back, and why.
#[derive(Debug, Clone, PartialEq)]
pub struct DroppedBand {
    pub centre_y: f32,
    pub lines: usize,
    /// Why it was held back. Every drop is reported to the caller - nothing is
    /// discarded silently - but `Verdict::is_worth_reporting` says which are
    /// worth a log line, since a glimpse is held every few seconds all session
    /// and logging each would bury the drops that explain a missing subtitle.
    pub verdict: Verdict,
}

/// One frame's lines, sorted into what to translate and what to leave.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Banding {
    /// Ordered top to bottom, so a caller placing translations does not have to
    /// sort them itself.
    pub included: Vec<BandGroup>,
    pub dropped: Vec<DroppedBand>,
}
