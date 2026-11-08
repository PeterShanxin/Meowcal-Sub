use super::*;
use crate::ipc::{IpcMessage, SubtitleUpdatePayload};
use crate::subtitle_queue::{Display, QueueStats};

impl Translator {
    pub(super) async fn display_loop(
        &self,
        mut stop: watch::Receiver<bool>,
        mut shutdown: watch::Receiver<bool>,
    ) {
        let mut tick = tokio::time::interval(Duration::from_millis(25));
        tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        let mut reported = Instant::now();
        let mut previous = QueueStats::default();
        loop {
            tokio::select! {
                biased;
                _ = stop.changed() => break,
                _ = shutdown.changed() => break,
                _ = tick.tick() => {},
            }
            if *stop.borrow()
                || *shutdown.borrow()
                || !self.clock.is_session_current(self.session_id)
            {
                break;
            }
            let now = Instant::now();
            let ipc = {
                let mut queue = lock_or_recover(&self.queue);
                if !self.clock.is_session_current(self.session_id) {
                    break;
                }
                let action = queue.next_display(now);
                let stats = queue.stats(now);
                if reported.elapsed() >= Duration::from_secs(5) {
                    let elapsed = reported.elapsed().as_secs_f64();
                    info!(
                        session_id = self.session_id,
                        depth = stats.depth,
                        oldest_ms = stats.oldest_ms,
                        enqueued = stats.enqueued,
                        completed = stats.completed,
                        displayed = stats.displayed,
                        skipped = stats.skipped,
                        input_per_second = (stats.enqueued - previous.enqueued) as f64 / elapsed,
                        output_per_second = (stats.displayed - previous.displayed) as f64 / elapsed,
                        "pipeline_queue"
                    );
                    if stats.depth > 3 || stats.oldest_ms > 3_000 {
                        warn!(
                            session_id = self.session_id,
                            depth = stats.depth,
                            oldest_ms = stats.oldest_ms,
                            "Subtitle throughput is falling behind"
                        );
                    }
                    previous = stats;
                    reported = now;
                }
                match action {
                    Some(Display::Show {
                        job,
                        mut result,
                        source_ms,
                    }) => {
                        if !self
                            .clock
                            .is_translation_current(job.frame.region_generation)
                        {
                            queue.reset();
                            self.generation.send_replace(queue.generation());
                            continue;
                        }
                        result.timestamp = timestamp();
                        result.total_ms =
                            now.saturating_duration_since(job.frame.started).as_millis() as u64;
                        result.cue_id = job.cue_id;
                        result.update_id = self.clock.next_update();
                        let translated =
                            result.display_state == TranslationDisplayState::Translated;
                        let clear = matches!(
                            result.display_state,
                            TranslationDisplayState::SourceOnly
                                | TranslationDisplayState::Stopped
                                | TranslationDisplayState::NoSubtitleText
                        );
                        let ipc = (translated || clear).then(|| {
                            IpcMessage::with_payload(
                                "Subtitle.Update",
                                SubtitleUpdatePayload {
                                    text: if clear {
                                        String::new()
                                    } else {
                                        result.translated.clone()
                                    },
                                    source_text: if clear {
                                        String::new()
                                    } else {
                                        result.original.clone()
                                    },
                                    timestamp: result.timestamp.to_string(),
                                    backend_used: Some(result.backend_used.clone()),
                                },
                            )
                        });
                        info!(
                            session_id = self.session_id,
                            cue_id = job.cue_id,
                            capture_id = job.frame.token.capture_id,
                            capture_ms = job.frame.capture_ms,
                            sample_ms = job.frame.sample_ms,
                            ocr_ms = job.frame.ocr_ms,
                            model_ms = result.model_ms,
                            queue_ms = job.queue_ms,
                            source_ms,
                            total_ms = result.total_ms,
                            depth = stats.depth,
                            translated,
                            "pipeline_frame_complete"
                        );
                        if let Err(error) = self.app.emit("translation-update", result) {
                            warn!(%error, "Failed to emit subtitle");
                        }
                        ipc
                    }
                    Some(Display::Clear) => {
                        let mut payload = TranslationPayload::no_subtitle_text(self.session_id, 0);
                        payload.update_id = self.clock.next_update();
                        let _ = self.app.emit("translation-update", payload);
                        Some(IpcMessage::with_payload(
                            "Subtitle.Update",
                            SubtitleUpdatePayload {
                                text: String::new(),
                                source_text: String::new(),
                                timestamp: timestamp().to_string(),
                                backend_used: None,
                            },
                        ))
                    }
                    None => None,
                }
            };
            if let Some(message) = ipc {
                crate::overlay_ipc::send_overlay_message(&self.app, message).await;
            }
        }
    }
}
