use super::{frame_bytes, geometry, NativeResult, OcrError};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::time::Duration;
use windows::Globalization::Language;
use windows::Graphics::Imaging::{BitmapAlphaMode, BitmapPixelFormat, SoftwareBitmap};
use windows::Media::Ocr::OcrEngine;
use windows::Security::Cryptography::CryptographicBuffer;

pub struct NativeOcr {
    engine: OcrEngine,
    timed_out: bool,
}

impl NativeOcr {
    pub fn language(&self) -> Result<String, OcrError> {
        self.engine
            .RecognizerLanguage()
            .and_then(|language| language.LanguageTag())
            .map(|tag| tag.to_string())
            .map_err(|e| OcrError::Init(e.to_string()))
    }

    pub fn is_poisoned(&self) -> bool {
        self.timed_out
    }

    pub fn new(language_tag: Option<&str>) -> Result<Self, OcrError> {
        let engine = match language_tag {
            None => OcrEngine::TryCreateFromUserProfileLanguages()
                .map_err(|e| OcrError::Init(e.to_string()))?,
            Some(tag) => {
                // Preserve an installed script-qualified tag supplied by a client.
                let tag = tag.trim().replace('_', "-");
                let language = Language::CreateLanguage(&tag.clone().into())
                    .map_err(|e| OcrError::Language(e.to_string()))?;
                if !OcrEngine::IsLanguageSupported(&language)
                    .map_err(|e| OcrError::Init(e.to_string()))?
                {
                    return Err(OcrError::Language(tag));
                }
                OcrEngine::TryCreateFromLanguage(&language)
                    .map_err(|e| OcrError::Init(e.to_string()))?
            }
        };
        Ok(Self {
            engine,
            timed_out: false,
        })
    }

    pub fn available_languages() -> Result<Vec<String>, OcrError> {
        let languages =
            OcrEngine::AvailableRecognizerLanguages().map_err(|e| OcrError::Init(e.to_string()))?;
        let mut tags = Vec::new();
        for i in 0..languages
            .Size()
            .map_err(|e| OcrError::Init(e.to_string()))?
        {
            let language = languages
                .GetAt(i)
                .map_err(|e| OcrError::Init(e.to_string()))?;
            tags.push(
                language
                    .LanguageTag()
                    .map_err(|e| OcrError::Init(e.to_string()))?
                    .to_string(),
            );
        }
        Ok(tags)
    }

    pub fn recognize_bgra(
        &mut self,
        bytes: &[u8],
        width: u32,
        height: u32,
        timeout: Duration,
    ) -> Result<NativeResult, OcrError> {
        if self.timed_out {
            return Err(OcrError::Timeout);
        }
        let expected = frame_bytes(width, height, width.saturating_mul(4))?;
        if bytes.len() != expected {
            return Err(OcrError::Frame(
                "byte length does not match dimensions".into(),
            ));
        }
        let max = OcrEngine::MaxImageDimension().map_err(|e| OcrError::Init(e.to_string()))?;
        if width > max || height > max {
            return Err(OcrError::Frame(format!(
                "dimensions exceed native limit {max}"
            )));
        }
        let buffer = CryptographicBuffer::CreateFromByteArray(bytes)
            .map_err(|e| OcrError::Frame(e.to_string()))?;
        let bitmap = SoftwareBitmap::CreateCopyWithAlphaFromBuffer(
            &buffer,
            BitmapPixelFormat::Bgra8,
            width as i32,
            height as i32,
            BitmapAlphaMode::Ignore,
        )
        .map_err(|e| OcrError::Frame(e.to_string()))?;
        let operation = self
            .engine
            .RecognizeAsync(&bitmap)
            .map_err(|e| OcrError::Recognition(e.to_string()))?;
        let (completed, completion) = mpsc::sync_channel(1);
        let handler = windows_future::AsyncOperationCompletedHandler::new(move |_, _| {
            let _ = completed.try_send(());
            Ok(())
        });
        if let Err(error) = operation.SetCompleted(&handler) {
            let _ = operation.Cancel();
            self.timed_out = true;
            return Err(OcrError::Recognition(error.to_string()));
        }
        if let Err(error) = wait_for_completion(timeout, &completion, || {
            let _ = operation.Cancel();
        }) {
            // Cancellation is best effort. Poison the engine so another call
            // cannot stack an operation behind native work that ignored it.
            self.timed_out = true;
            return Err(error);
        }
        let result = operation
            .GetResults()
            .map_err(|e| OcrError::Recognition(e.to_string()))?;
        let text = result
            .Text()
            .map_err(|e| OcrError::Recognition(e.to_string()))?
            .to_string();
        let (lines, boxes) = geometry::lines_with_boxes(&result);
        Ok(NativeResult {
            text,
            lines,
            boxes,
            frame_width: width as f32,
        })
    }
}

fn wait_for_completion(
    timeout: Duration,
    complete: &Receiver<()>,
    cancel: impl FnOnce(),
) -> Result<(), OcrError> {
    match complete.recv_timeout(timeout) {
        Ok(()) => Ok(()),
        Err(error) => {
            cancel();
            Err(match error {
                RecvTimeoutError::Timeout => OcrError::Timeout,
                RecvTimeoutError::Disconnected => {
                    OcrError::Recognition("Native completion notification was lost".into())
                }
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    #[test]
    fn a_native_operation_ignoring_cancel_still_releases_the_caller() {
        let cancels = Cell::new(0);
        let (_sender, completion) = mpsc::sync_channel(1);
        let result = wait_for_completion(Duration::ZERO, &completion, || {
            cancels.set(cancels.get() + 1)
        });
        assert!(matches!(result, Err(OcrError::Timeout)));
        assert_eq!(cancels.get(), 1);
    }

    #[test]
    fn completion_is_checked_before_the_deadline_without_cancel() {
        let cancels = Cell::new(0);
        let (sender, completion) = mpsc::sync_channel(1);
        sender.send(()).unwrap();
        assert!(wait_for_completion(Duration::ZERO, &completion, || cancels.set(1)).is_ok());
        assert_eq!(cancels.get(), 0);
    }

    #[test]
    fn lost_completion_notification_requests_cancellation() {
        let cancels = Cell::new(0);
        let (sender, completion) = mpsc::sync_channel(1);
        drop(sender);
        let result = wait_for_completion(Duration::ZERO, &completion, || cancels.set(1));
        assert!(matches!(result, Err(OcrError::Recognition(_))));
        assert_eq!(cancels.get(), 1);
    }

    #[test]
    fn windows_ocr_recognizes_a_blank_frame() {
        assert!(!NativeOcr::available_languages().unwrap().is_empty());
        let mut engine = NativeOcr::new(None).unwrap();
        let result = engine
            .recognize_bgra(&vec![255; 100 * 100 * 4], 100, 100, Duration::from_secs(5))
            .unwrap();
        assert!(result.text.trim().is_empty());
        assert_eq!(result.lines.len(), result.boxes.len());
    }

    #[test]
    fn native_language_probe_accepts_supported_bcp47_and_rejects_unavailable_language() {
        let engine = NativeOcr::new(Some("en")).unwrap();
        assert!(engine.language().unwrap().starts_with("en"));
        assert!(NativeOcr::new(Some("zz-ZZ")).is_err());
    }
}
