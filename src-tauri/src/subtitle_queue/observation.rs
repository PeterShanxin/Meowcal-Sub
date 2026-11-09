use super::*;

impl<R> SubtitleQueue<R> {
    /// Ineligible observations can keep a paused cue alive, but cannot create
    /// work. Two consecutive readings confirm new text, without fuzzy dedupe.
    pub fn observe(&mut self, frame: Frame, eligible: bool, now: Instant) -> Option<u64> {
        let observed_at = frame.started;
        let key = identity(&frame.text);
        if key.is_empty() {
            self.candidate = None;
            let since = *self.quiet_since.get_or_insert(frame.started);
            if observed_at.saturating_duration_since(since) >= self.quiet_for {
                self.end_active(since);
            }
            return None;
        }
        self.quiet_since = None;
        if let Some(active) = &mut self.active {
            if active.key == key {
                active.last_seen = frame.started;
                self.candidate = None;
                if active.retry_at.is_some_and(|at| now >= at)
                    && !self
                        .entries
                        .iter()
                        .any(|entry| entry.job.cue_id == active.id)
                {
                    let id = active.id;
                    active.retry_at = None;
                    self.enqueue(id, frame, now);
                    return Some(id);
                }
                return None;
            }
        }
        let confirmed = self.candidate.as_ref().is_some_and(|(pending, first)| {
            pending == &key
                && frame.started > first.started
                && frame.started.saturating_duration_since(first.started) <= Duration::from_secs(2)
        });
        if confirmed && eligible {
            let (_, mut first) = self.candidate.take()?;
            first.text = frame.text;
            self.end_active(first.started);
            self.next_id += 1;
            let id = self.next_id;
            self.active = Some(Active {
                id,
                key,
                last_seen: frame.started,
                retry_at: None,
            });
            self.enqueue(id, first, now);
            return Some(id);
        }
        if !confirmed {
            self.candidate = Some((key, frame));
        }
        if let Some(active) = &self.active {
            if confirmed
                && observed_at.saturating_duration_since(active.last_seen) >= self.quiet_for
            {
                let changed_at = self
                    .candidate
                    .as_ref()
                    .map_or(observed_at, |(_, first)| first.started);
                self.end_active(changed_at);
            }
        }
        None
    }

    fn enqueue(&mut self, id: u64, frame: Frame, now: Instant) {
        self.entries.push_back(Entry {
            job: Job {
                cue_id: id,
                generation: self.generation,
                frame,
                queue_ms: 0,
            },
            queued_at: now,
            end: None,
            in_flight: false,
            ready: None,
        });
        self.stats.enqueued += 1;
        self.recover_if_stalled(now);
    }

    fn end_active(&mut self, end: Instant) {
        if let Some(active) = self.active.take() {
            for entry in &mut self.entries {
                if entry.job.cue_id == active.id {
                    entry.end = Some(end);
                }
            }
            if let Some(visible) = &mut self.visible {
                if visible.id == active.id {
                    visible.end = Some(end);
                }
            }
        }
    }
}

/// Keep word boundaries, digits, apostrophes and decimal punctuation. OCR's
/// whitespace/case and trailing sentence punctuation do not create new cues.
fn identity(text: &str) -> String {
    text.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .trim_end_matches(['.', ',', '!', '?', ';', ':', '。', '，', '！', '？', '…'])
        .replace(['’', '‘'], "'")
        // PP-OCR alternates these equivalent forms on an unchanged Chinese
        // glyph. Keep other CJK character changes significant.
        .replace(['凈', '淨'], "净")
        .to_lowercase()
}
