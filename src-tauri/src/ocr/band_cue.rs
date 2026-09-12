//! Content identity for one spatial band. Geometry never identifies a cue.

/// The existing 55 cues/minute ceiling, expressed as a minimum cue duration.
const MIN_CUE_MS: u64 = (60_000.0 / super::band_verdict::MAX_CUE_RATE_PER_MINUTE) as u64;
/// A held reading may stop being admitted, without banning its screen position.
const STATIC_MS: u64 = (60_000.0 / super::band_verdict::MIN_CUE_RATE_PER_MINUTE) as u64;

#[derive(Debug, Default)]
pub(super) struct Cue {
    anchor: String,
    pending: String,
    pending_since: u64,
    pending_observed_ms: u64,
    started: u64,
    started_observed_ms: u64,
    observed_ms: u64,
    last_at: Option<u64>,
    last_text: String,
    absent: bool,
    fast_change: bool,
    numeric_changes: usize,
    pub(super) id: u64,
    pub(super) confirmed: bool,
    pub(super) changed: bool,
}

impl Cue {
    pub(super) fn observe(&mut self, text: &str, at_ms: u64, max_gap_ms: u64) {
        let text = normalize(text);
        if let Some(last) = self.last_at {
            let gap = at_ms.saturating_sub(last);
            if gap <= max_gap_ms {
                self.observed_ms += gap;
            } else {
                self.pending.clear();
                self.last_text.clear();
            }
        }
        self.changed = false;
        self.confirmed = false;
        if text.is_empty() {
            self.missing();
            return;
        }
        if !self.absent && text == self.anchor {
            self.pending.clear();
            self.confirmed = true;
        } else if (text == self.pending
            || text == self.last_text
            || (similar(&self.pending, &text)
                && !similar(&self.anchor, &text)
                && !similar(&self.anchor, &self.pending)))
            && self.last_at.is_some_and(|last| at_ms > last)
        {
            // An anchor-like first misread cannot pin the candidate forever:
            // two subsequent exact readings establish their own candidate.
            if text == self.last_text && text != self.pending {
                self.pending.clone_from(&text);
                self.pending_since = self.last_at.unwrap_or(at_ms);
                self.pending_observed_ms = self
                    .observed_ms
                    .saturating_sub(at_ms.saturating_sub(self.pending_since));
            }
            self.fast_change = self.id > 0
                && !self.absent
                && self
                    .pending_observed_ms
                    .saturating_sub(self.started_observed_ms)
                    < MIN_CUE_MS;
            self.numeric_changes = if !self.absent
                && numeric_template(&self.anchor) == numeric_template(&self.pending)
                && self
                    .pending_observed_ms
                    .saturating_sub(self.started_observed_ms)
                    <= 2 * MIN_CUE_MS
            {
                self.numeric_changes + 1
            } else {
                0
            };
            self.anchor = std::mem::take(&mut self.pending);
            self.started = self.pending_since;
            self.started_observed_ms = self.pending_observed_ms;
            self.id += 1;
            self.absent = false;
            self.confirmed = true;
            self.changed = true;
        } else {
            // Always compare with the committed anchor. Pairwise comparison
            // would let a sequence of small OCR changes drift into a new cue.
            self.confirmed = !self.absent && similar(&self.anchor, &text);
            if !similar(&self.pending, &text) {
                self.pending.clone_from(&text);
                self.pending_since = at_ms;
                self.pending_observed_ms = self.observed_ms;
            }
        }
        self.last_text = text;
        self.last_at = Some(at_ms);
    }

    pub(super) fn missing(&mut self) {
        self.absent = true;
        self.pending.clear();
        self.confirmed = false;
        self.changed = false;
        self.last_at = None;
        self.last_text.clear();
    }

    pub(super) fn verdict(&self, at_ms: u64) -> super::band_verdict::Verdict {
        use super::band_verdict::Verdict;
        if !self.confirmed || self.id == 0 {
            return Verdict::Glimpsed;
        }
        let age = at_ms.saturating_sub(self.started);
        if age >= STATIC_MS {
            Verdict::Static
        } else if self.numeric_changes >= 2 || (self.fast_change && age < MIN_CUE_MS) {
            Verdict::Churning
        } else {
            Verdict::Subtitle
        }
    }
}

// Repeated numeric-only updates at up to twice the rapid-cue duration describe
// counters. Slower numeric dialogue and a single changed number remain eligible.
fn numeric_template(text: &str) -> String {
    let mut result = String::new();
    let mut number = false;
    for ch in text.chars() {
        if ch.is_numeric() {
            if !number {
                result.push('#');
            }
            number = true;
        } else {
            result.push(ch);
            number = false;
        }
    }
    result
}

fn normalize(text: &str) -> String {
    let chars: Vec<char> = text.chars().flat_map(char::to_lowercase).collect();
    chars
        .iter()
        .enumerate()
        .filter(|(i, c)| {
            c.is_alphanumeric()
                || ((**c == '.' || **c == ',')
                    && *i > 0
                    && chars[*i - 1].is_numeric()
                    && chars.get(*i + 1).is_some_and(|next| next.is_numeric()))
        })
        .map(|(_, c)| c)
        .collect()
}

/// A single transient wrong glyph in a long reading can still belong to the
/// anchor. Repeated identical changes are confirmed separately, even here.
/// Short text, digits, and insertions/deletions require exact confirmation.
fn similar(anchor: &str, text: &str) -> bool {
    let old: Vec<char> = anchor.chars().collect();
    let new: Vec<char> = text.chars().collect();
    old.len() >= 16
        && old.len() == new.len()
        && old.iter().zip(&new).filter(|(a, b)| a != b).count() == 1
        && old
            .iter()
            .zip(&new)
            .all(|(a, b)| a == b || (!a.is_numeric() && !b.is_numeric()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_candidate_cannot_drift_through_pairwise_similar_readings() {
        let mut cue = Cue::default();
        cue.observe("The morning train is arriving", 0, 2_000);
        cue.observe("The morning train is arriving", 250, 2_000);
        for (i, text) in [
            "The marning train is arriving",
            "The marning traim is arriving",
            "The marning traim is arriring",
        ]
        .iter()
        .enumerate()
        {
            cue.observe(text, 500 + i as u64 * 250, 2_000);
        }
        assert_eq!(cue.id, 1);
        assert!(!cue.confirmed);
        cue.observe("Please wait by the entrance", 1_250, 2_000);
        cue.observe("Please wait by the entrance", 1_500, 2_000);
        assert_eq!(cue.id, 2);
        assert!(cue.confirmed);
    }

    #[test]
    fn repeated_changes_are_not_lost_to_fuzzy_comparison() {
        let mut cue = Cue::default();
        for (i, text) in [
            "Please wait in room 12",
            "Please wait in room 13",
            "Please wait in room 1.3",
            "I can come to the meeting",
            "I can't come to the meeting",
            "我知道明天可以去开会",
            "我知道明天不可以去开会",
        ]
        .iter()
        .enumerate()
        {
            cue.observe(text, i as u64 * 4_000, 2_000);
            cue.observe(text, i as u64 * 4_000 + 250, 2_000);
            assert_eq!(cue.id, i as u64 + 1, "{text}");
        }
    }

    #[test]
    fn punctuation_and_spacing_do_not_restart_a_cue() {
        let mut cue = Cue::default();
        for (i, text) in [
            "Please wait here.",
            "Please wait here.",
            "Please  wait here!",
            "Pleasewait here",
        ]
        .iter()
        .enumerate()
        {
            cue.observe(text, i as u64 * 250, 2_000);
        }
        assert_eq!(cue.id, 1);
    }
}
