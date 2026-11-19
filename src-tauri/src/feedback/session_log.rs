//! Read a bounded tail of the exact log file opened by this process.
use super::trace::Frame;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

static LOG: OnceLock<(Mutex<File>, PathBuf)> = OnceLock::new();

pub(crate) fn register(path: &Path) {
    if let Ok(file) = File::open(path) {
        let _ = LOG.set((Mutex::new(file), path.to_path_buf()));
    }
}

fn related_line(line: &str, session: u64, capture: u64) -> Option<&str> {
    // Only accept numeric pipeline fields emitted by these exact call sites.
    // Raw backend messages, source locations and user text have no place in an excerpt.
    let (_, body) = line.split_once(": pipeline_")?;
    let (name, fields) = body.split_once(", ")?;
    if !matches!(name, "frame_complete" | "cue_enqueued" | "feedback_fault") {
        return None;
    }
    let mut matches_session = false;
    let mut matches_capture = false;
    for field in fields.split(", ") {
        let (key, value) = field.split_once(": ")?;
        match key {
            "translated" if matches!(value, "true" | "false") => {}
            "session_id" => matches_session = value.parse::<u64>().ok() == Some(session),
            "capture_id" => matches_capture = value.parse::<u64>().ok() == Some(capture),
            "cue_id" | "source_chars" | "depth" | "capture_ms" | "sample_ms" | "ocr_ms"
            | "model_ms" | "queue_ms" | "source_ms" | "total_ms"
                if value.parse::<u64>().is_ok() => {}
            _ => return None,
        }
    }
    (matches_session && matches_capture)
        .then_some(&line[line.len() - body.len() - "pipeline_".len()..])
}

pub(super) fn excerpt(frame: &Frame) -> Option<(String, String)> {
    if frame.token.session_id == 0 {
        return None;
    }
    let (file, path) = LOG.get()?;
    let mut file = crate::sync_utils::lock_or_recover(file);
    let size = file.metadata().ok()?.len();
    file.seek(SeekFrom::Start(size.saturating_sub(512 * 1024)))
        .ok()?;
    let mut bytes = Vec::new();
    (&mut *file).take(512 * 1024).read_to_end(&mut bytes).ok()?;
    let raw = String::from_utf8_lossy(&bytes);
    let lines = raw
        .lines()
        .filter_map(|line| related_line(line, frame.token.session_id, frame.token.capture_id))
        .take(12)
        .collect::<Vec<_>>()
        .join("\n");
    (!lines.is_empty()).then(|| (lines, path.to_string_lossy().into_owned()))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_pretty_formatter_excerpt_excludes_source_locations() -> std::io::Result<()> {
        let path = std::env::temp_dir().join(format!("feedback-log-{}.txt", uuid::Uuid::new_v4()));
        let writer = File::create_new(&path)?;
        let subscriber = tracing_subscriber::fmt()
            .pretty()
            .with_ansi(false)
            .with_writer(writer)
            .finish();
        tracing::subscriber::with_default(subscriber, || {
            tracing::info!(
                session_id = 21_u64,
                capture_id = 53_u64,
                translated = true,
                "pipeline_frame_complete"
            );
        });
        let content = std::fs::read_to_string(&path)?;
        std::fs::remove_file(path)?;
        let excerpt = content.lines().find_map(|line| related_line(line, 21, 53));
        assert_eq!(
            excerpt,
            Some("pipeline_frame_complete, session_id: 21, capture_id: 53, translated: true")
        );
        Ok(())
    }

    #[test]
    fn only_exact_frame_and_known_safe_fields_survive() {
        let line = "[prod] INFO meowcal_sub: pipeline_frame_complete, session_id: 1, capture_id: 8, translated: true";
        assert_eq!(
            related_line(line, 1, 8),
            Some("pipeline_frame_complete, session_id: 1, capture_id: 8, translated: true")
        );
        assert_eq!(related_line(line, 1, 80), None);
        assert_eq!(related_line(&format!("{line}, path: private"), 1, 8), None);
        assert_eq!(
            related_line(
                "engine: pipeline_frame_complete, session_id: 1, capture_id: 8, cue_id: subtitle",
                1,
                8
            ),
            None
        );
    }
}
