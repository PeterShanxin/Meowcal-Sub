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

fn safe_header(header: &str) -> bool {
    let mut parts = header.split_ascii_whitespace();
    let profile = parts.next();
    let timestamp = parts.next();
    let level = parts.next();
    let target = parts.next();
    matches!(profile, Some("[prod]" | "[dev]" | "[unknown]"))
        && timestamp
            .is_some_and(|v| v.len() <= 35 && chrono::DateTime::parse_from_rfc3339(v).is_ok())
        && matches!(level, Some("DEBUG" | "INFO" | "WARN" | "ERROR" | "TRACE"))
        && matches!(
            target,
            Some(
                "meowcal_sub::pipeline_translation"
                    | "meowcal_sub::pipeline_translation::presentation"
                    | "meowcal_sub::pipeline_status"
                    | "meowcal_sub::pipeline_ocr"
                    | "meowcal_sub::commands"
                    | "meowcal_sub::feedback::trace"
            )
        )
        && parts.next().is_none()
}

fn related_line(line: &str, session: u64, capture: u64, verbose: bool) -> Option<&str> {
    if line.len() > 1024 || line.bytes().any(|byte| byte.is_ascii_control()) {
        return None;
    }
    let (header, body) = line.split_once(": ")?;
    if !safe_header(header) {
        return None;
    }
    let (name, fields) = body.split_once(", session_id: ")?;
    match name {
        "pipeline_frame_complete" | "pipeline_cue_enqueued" | "pipeline_feedback_fault" => {}
        "Stale capture, before OCR"
        | "Stale capture, after OCR"
        | "subtitle_activity_changed"
        | "Translation missed its deadline"
        | "Subtitle band skipped"
        | "OCR text rejected"
            if verbose => {}
        _ => return None,
    }
    // An arbitrary backend message can contain a path, secret, or the subtitle.
    // Admit complete original lines only for known call sites and typed fields.
    let (actual_session, fields) = fields.split_once(", ")?;
    if actual_session.parse::<u64>().ok() != Some(session) {
        return None;
    }
    let mut matched = false;
    for field in fields.split(", ") {
        let (key, value) = field.split_once(": ")?;
        let literal = value.strip_prefix('"').and_then(|v| v.strip_suffix('"'));
        match key {
            "capture_id" if !matched && value.parse::<u64>().ok() == Some(capture) => {
                matched = true
            }
            "translated" if matches!(value, "true" | "false") => {}
            "state" if matches!(literal, Some("active" | "waiting")) => {}
            "stage" if matches!(literal, Some("capture" | "ocr" | "translation" | "overlay")) => {}
            "code"
                if matches!(
                    literal,
                    Some(
                        "CAPTURE_FAILED"
                            | "DISPLAY_CHANGED"
                            | "OCR_FAILED"
                            | "TRANSLATION_TIMEOUT"
                            | "TRANSLATION_FAILED"
                            | "OVERLAY_SEND_FAILED"
                    )
                ) => {}
            "reason"
                if matches!(
                    literal,
                    Some(
                        "empty"
                            | "band_held"
                            | "tooShort"
                            | "tooLong"
                            | "untranslatable"
                            | "garbled"
                            | "bandHeld"
                    )
                ) => {}
            "cue_id" | "source_chars" | "depth" | "capture_ms" | "sample_ms" | "ocr_ms"
            | "model_ms" | "queue_ms" | "source_ms" | "total_ms" | "waited_ms" | "min_chars"
                if value.parse::<u64>().is_ok() => {}
            _ => return None,
        }
    }
    matched.then_some(if verbose { line } else { body })
}

fn frame_lines(raw: &str, session: u64, capture: u64, verbose: bool) -> String {
    let mut lines: Vec<_> = raw
        .lines()
        .rev()
        .filter_map(|line| related_line(line, session, capture, verbose))
        .take(16)
        .collect();
    lines.reverse();
    lines.join("\n")
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
    let lines = frame_lines(
        &raw,
        frame.token.session_id,
        frame.token.capture_id,
        frame.verbose,
    );
    (!lines.is_empty()).then(|| (lines, path.to_string_lossy().into_owned()))
}

#[cfg(test)]
mod tests {
    use super::*;
    const HEADER: &str =
        "  [dev] 2026-11-19T12:34:56.123+08:00  WARN meowcal_sub::pipeline_translation: ";
    #[test]
    fn raw_preview_matches_the_actual_formatter_and_the_selected_archive_entry(
    ) -> std::io::Result<()> {
        let path = std::env::temp_dir().join(format!("feedback-log-{}.txt", uuid::Uuid::new_v4()));
        let writer = File::create_new(&path)?;
        let subscriber = tracing_subscriber::fmt()
            .pretty()
            .with_ansi(false)
            .with_timer(crate::app_logging::ProfileTimer)
            .with_writer(writer)
            .finish();
        tracing::subscriber::with_default(subscriber, || {
            tracing::warn!(target:"meowcal_sub::pipeline_translation",session_id=21_u64,cue_id=3_u64,capture_id=53_u64,waited_ms=7000_u64,"Translation missed its deadline");
            tracing::warn!(target:"meowcal_sub::pipeline_translation",session_id=21_u64,capture_id=53_u64,token="secret","untrusted detail");
        });
        let content = std::fs::read_to_string(&path)?;
        std::fs::remove_file(path)?;
        let line = content
            .lines()
            .find(|l| l.contains("Translation missed its deadline"))
            .unwrap();
        let raw = frame_lines(&content, 21, 53, true);
        assert_eq!(raw, line);
        assert!(frame_lines(&content, 21, 53, false).is_empty());
        let mut preview = super::super::Preview::from_frame(None);
        preview.materials.push(super::super::text_material(
            "log",
            "Session log excerpt",
            raw,
        ));
        assert_eq!(preview.files(&[]).unwrap().len(), 1);
        assert_eq!(
            preview.files(&["log".into()]).unwrap()[1].1,
            line.as_bytes()
        );
        Ok(())
    }

    #[test]
    fn raw_excerpt_excludes_other_frames_free_text_locations_and_credentials() {
        let allowed=format!("{HEADER}OCR text rejected, session_id: 1, capture_id: 8, reason: \"garbled\", source_chars: 9, min_chars: 4");
        let raw = [
            allowed.clone(),
            allowed.replace("capture_id: 8", "capture_id: 18"),
            allowed.replace("session_id: 1", "session_id: 9"),
            allowed.replace("\"garbled\"", "\"private subtitle\""),
            format!("{allowed}, path: C:/Users/name"),
            format!("{allowed}, token: sk-private"),
            format!("{HEADER}C:/Users/private: OCR text rejected, session_id: 1, capture_id: 8"),
            "    at C:/Users/name/private.rs:122".to_owned(),
        ]
        .join("\n");
        assert_eq!(frame_lines(&raw, 1, 8, true), allowed);
        assert!(frame_lines(&raw, 1, 8, false).is_empty());
        let repeated = std::iter::repeat_n(allowed.as_str(), 45)
            .collect::<Vec<_>>()
            .join("\n");
        assert_eq!(frame_lines(&repeated, 1, 8, true).lines().count(), 16);
    }
}
