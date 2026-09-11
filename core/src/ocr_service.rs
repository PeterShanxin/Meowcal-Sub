use crate::ocr::{frame_bytes, NativeOcr, OcrError};
use crate::protocol::Error;
use base64::Engine;
use serde::Deserialize;
use serde_json::{Map, Value};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

// An ignored native cancellation poisons the entire OCR process, including
// newly created engine objects. Only a replacement process can accept a frame.
static TIMED_OUT: AtomicBool = AtomicBool::new(false);

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Frame {
    language: Option<String>,
    width: u32,
    height: u32,
    stride: u32,
    bgra_base64: String,
    timeout_ms: u64,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Initialize {
    language: Option<String>,
}

impl Frame {
    fn decode(self) -> Result<(Self, Vec<u8>), Error> {
        let expected = frame_bytes(self.width, self.height, self.stride).map_err(map_error)?;
        if !(1..=30_000).contains(&self.timeout_ms) {
            return Err(Error::new(
                "INVALID_PARAMS",
                "timeoutMs must be within 1..=30000",
            ));
        }
        if self
            .language
            .as_ref()
            .is_some_and(|tag| tag.is_empty() || tag.len() > 128)
        {
            return Err(Error::new("INVALID_PARAMS", "Invalid language tag"));
        }
        if self.bgra_base64.len() != expected.div_ceil(3) * 4 {
            return Err(Error::new(
                "INVALID_IMAGE",
                "BGRA base64 length does not match dimensions",
            ));
        }
        let bytes = base64::prelude::BASE64_STANDARD
            .decode(&self.bgra_base64)
            .map_err(|_| Error::new("INVALID_IMAGE", "Invalid BGRA base64"))?;
        if bytes.len() != expected {
            return Err(Error::new(
                "INVALID_IMAGE",
                "BGRA byte length does not match dimensions",
            ));
        }
        Ok((self, bytes))
    }
}

pub async fn dispatch(method: &str, params: &Map<String, Value>) -> Result<Value, Error> {
    if TIMED_OUT.load(Ordering::Acquire) {
        return Err(map_error(OcrError::Timeout));
    }
    match method {
        "ocrInitialize" => {
            let options: Initialize = serde_json::from_value(Value::Object(params.clone()))
                .map_err(|_| Error::new("INVALID_PARAMS", "Invalid OCR language parameters"))?;
            if options
                .language
                .as_ref()
                .is_some_and(|tag| tag.is_empty() || tag.len() > 128)
            {
                return Err(Error::new("INVALID_PARAMS", "Invalid language tag"));
            }
            bounded_worker(Duration::from_secs(5), move || {
                let engine = NativeOcr::new(options.language.as_deref()).map_err(map_error)?;
                let language = engine.language().map_err(map_error)?;
                Ok(serde_json::json!({"resolvedLanguage": language}))
            })
            .await
        }
        "ocrLanguages" => {
            if !params.is_empty() {
                return Err(Error::new(
                    "INVALID_PARAMS",
                    "ocrLanguages takes no parameters",
                ));
            }
            bounded_worker(Duration::from_secs(5), || {
                NativeOcr::available_languages()
                    .map(|languages| serde_json::json!({"languages": languages}))
                    .map_err(map_error)
            })
            .await
        }
        "ocrRecognize" => {
            let frame: Frame = serde_json::from_value(Value::Object(params.clone()))
                .map_err(|_| Error::new("INVALID_PARAMS", "Invalid OCR frame parameters"))?;
            let (frame, bytes) = frame.decode()?;
            bounded_worker(Duration::from_millis(frame.timeout_ms + 1000), move || {
                let mut engine = NativeOcr::new(frame.language.as_deref()).map_err(map_error)?;
                match engine.recognize_bgra(
                    &bytes,
                    frame.width,
                    frame.height,
                    Duration::from_millis(frame.timeout_ms),
                ) {
                    Ok(result) => serde_json::to_value(result)
                        .map_err(|_| Error::new("OCR_ERROR", "OCR result serialization failed")),
                    Err(error) => {
                        if engine.is_poisoned() {
                            TIMED_OUT.store(true, Ordering::Release);
                        }
                        Err(map_error(error))
                    }
                }
            })
            .await
        }
        _ => Err(Error::new("UNKNOWN_METHOD", "Unknown OCR method")),
    }
}

async fn bounded_worker(
    timeout: Duration,
    work: impl FnOnce() -> Result<Value, Error> + Send + 'static,
) -> Result<Value, Error> {
    match tokio::time::timeout(timeout, tokio::task::spawn_blocking(work)).await {
        Ok(result) => result.map_err(|_| Error::new("OCR_ERROR", "OCR worker failed"))?,
        Err(_) => {
            // Core's stdio owner exits after this response, terminating even a
            // WinRT initialization call that cannot cooperate with cancellation.
            TIMED_OUT.store(true, Ordering::Release);
            Err(map_error(OcrError::Timeout))
        }
    }
}

fn map_error(error: OcrError) -> Error {
    let code = match error {
        OcrError::Init(_) => "OCR_INIT_FAILED",
        OcrError::Language(_) => "OCR_LANGUAGE_UNAVAILABLE",
        OcrError::Frame(_) => "INVALID_IMAGE",
        OcrError::Recognition(_) => "OCR_ERROR",
        OcrError::Timeout => "OCR_TIMEOUT",
    };
    Error::new(code, &error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(width: u32, stride: u32, bytes: &[u8]) -> Frame {
        Frame {
            language: None,
            width,
            height: 1,
            stride,
            bgra_base64: base64::prelude::BASE64_STANDARD.encode(bytes),
            timeout_ms: 1000,
        }
    }

    #[test]
    fn validates_frame_before_starting_native_work() {
        assert!(frame(1, 4, &[0; 4]).decode().is_ok());
        assert_eq!(
            frame(1, 4, &[0; 3]).decode().unwrap_err().code,
            "INVALID_IMAGE"
        );
        assert_eq!(
            frame(1, 8, &[0; 8]).decode().unwrap_err().code,
            "INVALID_IMAGE"
        );
        let mut invalid = frame(1, 4, &[0; 4]);
        invalid.timeout_ms = 0;
        assert_eq!(invalid.decode().unwrap_err().code, "INVALID_PARAMS");
    }
}
