//! Spatial history and admission for one band; text identity belongs to `Cue`.

use super::band_cue::Cue;
use super::band_verdict::{BandStats, Verdict};
use std::collections::VecDeque;

pub(super) const WINDOW_MS: u64 = 90_000;
/// Brief spatial noise must not demote an established subtitle band.
const SCATTER_HOLD_MS: u64 = 2_000;

#[derive(Debug)]
struct Observation {
    at_ms: u64,
    left: f32,
    right: f32,
    cue: u64,
    elapsed_ms: u64,
}

#[derive(Debug)]
pub(super) struct TrackedBand {
    pub(super) id: u64,
    centre_y: f32,
    last_seen_ms: u64,
    present: bool,
    max_gap_ms: u64,
    window: VecDeque<Observation>,
    pub(super) cue: Cue,
    pub(super) settled: Option<Verdict>,
    pub(super) last_raw: Option<Verdict>,
    scattered_since: Option<u64>,
}

impl TrackedBand {
    pub(super) fn new(centre_y: f32, interval_ms: u64) -> Self {
        Self {
            id: 0,
            centre_y,
            last_seen_ms: 0,
            present: false,
            max_gap_ms: interval_ms.saturating_mul(2).max(2_000),
            window: VecDeque::new(),
            cue: Cue::default(),
            settled: None,
            last_raw: None,
            scattered_since: None,
        }
    }

    pub(super) fn settle(&mut self, raw: Verdict, at_ms: u64) -> Verdict {
        let content = self.cue.verdict(at_ms);
        let verdict = if raw == Verdict::Scattered {
            let since = *self.scattered_since.get_or_insert(at_ms);
            if self.settled == Some(Verdict::Subtitle)
                && at_ms.saturating_sub(since) < SCATTER_HOLD_MS
            {
                content
            } else {
                Verdict::Scattered
            }
        } else {
            self.scattered_since = None;
            // Two agreeing text observations establish a candidate, including
            // initial acquisition. Historical turnover cannot veto that cue.
            content
        };
        self.settled = Some(verdict);
        verdict
    }

    pub(super) fn centre_y(&self) -> f32 {
        self.centre_y
    }

    pub(super) fn is_stale(&self, at_ms: u64, retire_ms: u64) -> bool {
        at_ms.saturating_sub(self.last_seen_ms) > retire_ms
    }

    pub(super) fn missing(&mut self) {
        self.present = false;
        self.cue.missing();
    }

    pub(super) fn record(&mut self, centre_y: f32, left: f32, right: f32, text: &str, at_ms: u64) {
        let gap = at_ms.saturating_sub(self.last_seen_ms);
        let elapsed_ms = if self.present && gap <= self.max_gap_ms {
            gap
        } else {
            0
        };
        self.centre_y = self.centre_y * 0.9 + centre_y * 0.1;
        self.last_seen_ms = at_ms;
        self.present = true;
        self.cue.observe(text, at_ms, self.max_gap_ms);
        self.window.push_back(Observation {
            at_ms,
            left,
            right,
            cue: self.cue.id,
            elapsed_ms,
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
        let centres: Vec<f32> = observations
            .iter()
            .map(|o| (o.left + o.right) / 2.0)
            .collect();
        let rights: Vec<f32> = observations.iter().map(|o| o.right).collect();
        let cues = usize::from(observations.first().is_some_and(|o| o.cue > 0))
            + observations
                .windows(2)
                .filter(|pair| pair[1].cue > 0 && pair[0].cue != pair[1].cue)
                .count();
        BandStats {
            observations: observations.len(),
            centre_scatter: scatter(&lefts).min(scatter(&centres)).min(scatter(&rights)),
            cues,
            on_screen_ms: observations.iter().map(|o| o.elapsed_ms).sum::<u64>()
                + frame_interval_ms,
        }
    }
}

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
