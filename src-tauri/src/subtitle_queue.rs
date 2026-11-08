//! One subtitle identity from stable OCR through FIFO translation and display.

use crate::pipeline_session::PipelineToken;
use std::collections::VecDeque;
use std::time::{Duration, Instant};

const MAX_DEPTH: usize = 32;
const MAX_AGE: Duration = Duration::from_secs(15);
#[path = "subtitle_queue/observation.rs"]
mod observation;

const RETRY_AFTER: Duration = Duration::from_millis(2_500);

#[derive(Clone, Debug)]
pub struct Frame {
    pub token: PipelineToken,
    pub region_generation: u64,
    pub text: String,
    pub context_prompt: Option<String>,
    pub started: Instant,
    pub capture_ms: u64,
    pub ocr_ms: u64,
    pub sample_ms: u64,
}

#[derive(Clone)]
pub struct Job {
    pub cue_id: u64,
    pub generation: u64,
    pub frame: Frame,
    pub queue_ms: u64,
}

struct Active {
    id: u64,
    key: String,
    last_seen: Instant,
    retry_at: Option<Instant>,
}

struct Entry<R> {
    job: Job,
    queued_at: Instant,
    end: Option<Instant>,
    in_flight: bool,
    ready: Option<Completion<R>>,
}

struct Visible {
    id: u64,
    source_started: Instant,
    end: Option<Instant>,
    shown_at: Instant,
    sample_ms: u64,
    text: String,
}

impl Visible {
    fn readable(&self, now: Instant) -> bool {
        // A sampled cue can appear one interval longer than the source.
        // Reserve that uncertainty so fast dialogue does not accumulate.
        let source_ms = self
            .end
            .unwrap_or(now)
            .saturating_duration_since(self.source_started)
            .as_millis() as u64;
        let delay_ms = self
            .shown_at
            .saturating_duration_since(self.source_started)
            .as_millis() as u64;
        now.saturating_duration_since(self.shown_at)
            >= crate::subtitle_timing::reading_time(
                source_ms.saturating_sub(self.sample_ms),
                &self.text,
                delay_ms,
            )
    }
}

#[derive(Clone, Copy)]
pub enum Presentation {
    Translated,
    Preserve,
    Clear,
}

pub struct Completion<R> {
    pub result: R,
    pub text: String,
    pub presentation: Presentation,
    pub retryable: bool,
}

pub enum Display<R> {
    Show { job: Job, result: R, source_ms: u64 },
    Clear,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct QueueStats {
    pub enqueued: u64,
    pub completed: u64,
    pub displayed: u64,
    pub skipped: u64,
    pub depth: usize,
    pub oldest_ms: u64,
}

pub struct SubtitleQueue<R> {
    entries: VecDeque<Entry<R>>,
    active: Option<Active>,
    candidate: Option<(String, Frame)>,
    quiet_since: Option<Instant>,
    quiet_for: Duration,
    visible: Option<Visible>,
    clear_pending: bool,
    next_id: u64,
    generation: u64,
    stats: QueueStats,
}

impl<R> SubtitleQueue<R> {
    pub fn new(interval: Duration) -> Self {
        Self {
            entries: VecDeque::new(),
            active: None,
            candidate: None,
            quiet_since: None,
            quiet_for: interval
                .saturating_mul(2)
                .clamp(Duration::from_millis(250), Duration::from_millis(900)),
            visible: None,
            clear_pending: false,
            next_id: 0,
            generation: 0,
            stats: QueueStats::default(),
        }
    }

    pub fn generation(&self) -> u64 {
        self.generation
    }

    pub fn reset(&mut self) {
        self.entries.clear();
        self.active = None;
        self.candidate = None;
        self.quiet_since = None;
        self.visible = None;
        self.clear_pending = true;
        self.generation += 1;
    }

    /// Recovery is exceptional and counted. Ordinary overload remains visible
    /// in depth/age metrics; it never silently turns into dropped dialogue.
    pub fn recover_if_stalled(&mut self, now: Instant) {
        let overdue = self
            .entries
            .front()
            .is_some_and(|entry| now.saturating_duration_since(entry.queued_at) >= MAX_AGE);
        if self.entries.len() < MAX_DEPTH && !overdue {
            return;
        }
        let current = self.active.as_ref().map(|active| active.id);
        if self.entries.len() == 1
            && self
                .entries
                .front()
                .is_some_and(|entry| Some(entry.job.cue_id) == current)
        {
            return;
        }
        let before = self.entries.len();
        self.entries
            .retain(|entry| Some(entry.job.cue_id) == current);
        self.stats.skipped += (before - self.entries.len()) as u64;
        self.generation += 1;
        for entry in &mut self.entries {
            entry.job.generation = self.generation;
            entry.in_flight = false;
        }
        self.visible = None;
        self.clear_pending = true;
    }

    pub fn next_job(&mut self, now: Instant) -> Option<Job> {
        self.recover_if_stalled(now);
        if self
            .entries
            .iter()
            .any(|entry| entry.in_flight && entry.ready.is_none())
        {
            return None;
        }
        let entry = self
            .entries
            .iter_mut()
            .find(|entry| !entry.in_flight && entry.ready.is_none())?;
        entry.in_flight = true;
        entry.job.queue_ms = now.saturating_duration_since(entry.queued_at).as_millis() as u64;
        Some(entry.job.clone())
    }

    pub fn complete(&mut self, job: &Job, completion: Completion<R>, now: Instant) -> bool {
        if job.generation != self.generation {
            return false;
        }
        let Some(entry) = self
            .entries
            .iter_mut()
            .find(|entry| entry.job.cue_id == job.cue_id)
        else {
            return false;
        };
        let retryable = completion.retryable;
        entry.ready = Some(completion);
        if retryable {
            if let Some(active) = &mut self.active {
                if active.id == job.cue_id {
                    active.retry_at = Some(now + RETRY_AFTER);
                }
            }
        }
        self.stats.completed += 1;
        true
    }

    pub fn next_display(&mut self, now: Instant) -> Option<Display<R>> {
        let can_replace = self
            .visible
            .as_ref()
            .is_none_or(|visible| visible.readable(now));
        if can_replace
            && self
                .entries
                .front()
                .is_some_and(|entry| entry.ready.is_some())
        {
            let mut entry = self.entries.pop_front()?;
            let Completion {
                result,
                text,
                presentation,
                ..
            } = entry.ready.take()?;
            if matches!(presentation, Presentation::Translated) {
                self.visible = Some(Visible {
                    id: entry.job.cue_id,
                    source_started: entry.job.frame.started,
                    end: entry.end,
                    shown_at: now,
                    sample_ms: entry.job.frame.sample_ms,
                    text,
                });
                self.clear_pending = false;
                self.stats.displayed += 1;
            } else if matches!(presentation, Presentation::Clear) {
                self.visible = None;
                self.clear_pending = false;
            }
            let source_ms = entry
                .end
                .unwrap_or(now)
                .saturating_duration_since(entry.job.frame.started)
                .as_millis() as u64;
            return Some(Display::Show {
                job: entry.job,
                result,
                source_ms,
            });
        }
        if can_replace
            && (self.entries.is_empty()
                || self.visible.as_ref().is_some_and(|v| {
                    v.end.is_some_and(|end| {
                        now.saturating_duration_since(end) >= Duration::from_secs(4)
                    })
                }))
            && self
                .visible
                .as_ref()
                .is_some_and(|visible| visible.end.is_some())
        {
            self.visible = None;
            self.clear_pending = true;
        }
        if self.clear_pending {
            self.clear_pending = false;
            return Some(Display::Clear);
        }
        None
    }

    pub fn is_busy(&self) -> bool {
        !self.entries.is_empty() || self.visible.is_some() || self.clear_pending
    }

    pub fn stats(&self, now: Instant) -> QueueStats {
        QueueStats {
            depth: self.entries.len(),
            oldest_ms: self.entries.front().map_or(0, |entry| {
                now.saturating_duration_since(entry.queued_at).as_millis() as u64
            }),
            ..self.stats
        }
    }
}

#[cfg(test)]
#[path = "subtitle_queue_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "subtitle_queue_recovery_tests.rs"]
mod recovery_tests;
