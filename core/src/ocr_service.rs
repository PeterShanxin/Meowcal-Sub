use crate::ocr::{frame_bytes, Engine, NativeOcr, OcrError, PpOcr};
use crate::protocol::Error;
use serde::Deserialize;
use serde_json::{Map, Value};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

#[derive(Default)]
pub struct OcrService {
    worker: Arc<Mutex<Worker>>,
    unusable: Arc<AtomicBool>,
}

#[derive(Default)]
struct Worker {
    windows: Option<CachedWindowsOcr>,
    pp_ocr: Option<PpOcr>,
}

struct CachedWindowsOcr {
    requested_language: Option<String>,
    engine: NativeOcr,
}

impl Worker {
    fn windows(&mut self, language: Option<&str>) -> Result<&mut NativeOcr, Error> {
        if self
            .windows
            .as_ref()
            .is_none_or(|cached| cached.requested_language.as_deref() != language)
        {
            self.windows = None;
            let engine = NativeOcr::new(language).map_err(map_error)?;
            engine.language().map_err(map_error)?;
            self.windows = Some(CachedWindowsOcr {
                requested_language: language.map(str::to_owned),
                engine,
            });
        }
        self.windows
            .as_mut()
            .map(|cached| &mut cached.engine)
            .ok_or_else(unusable_error)
    }

    fn pp_ocr(&mut self) -> Result<&PpOcr, Error> {
        if self.pp_ocr.is_none() {
            self.pp_ocr = Some(PpOcr::new().map_err(map_error)?);
        }
        self.pp_ocr.as_ref().ok_or_else(unusable_error)
    }

    fn clear(&mut self) {
        self.windows = None;
        self.pp_ocr = None;
    }
}

fn unusable_error() -> Error {
    Error::new(
        "OCR_PROCESS_UNUSABLE",
        "OCR worker state is unusable; restart Core",
    )
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Frame {
    #[serde(default)]
    engine: Engine,
    language: Option<String>,
    width: u32,
    height: u32,
    stride: u32,
    timeout_ms: u64,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Initialize {
    #[serde(default)]
    engine: Engine,
    language: Option<String>,
}

fn validate_language(engine: Engine, language: Option<&str>) -> Result<(), Error> {
    if language.is_some_and(|tag| tag.is_empty() || tag.len() > 128) {
        return Err(Error::new("INVALID_PARAMS", "Invalid language tag"));
    }
    if engine == Engine::PpOcrV6 && language.is_some() {
        return Err(Error::new("INVALID_PARAMS", "PP-OCRv6 takes no language"));
    }
    Ok(())
}

impl Frame {
    fn validate(&self, payload_bytes: u64) -> Result<usize, Error> {
        let expected = frame_bytes(self.width, self.height, self.stride).map_err(map_error)?;
        if !(1..=30_000).contains(&self.timeout_ms) {
            return Err(Error::new(
                "INVALID_PARAMS",
                "timeoutMs must be within 1..=30000",
            ));
        }
        validate_language(self.engine, self.language.as_deref())?;
        if payload_bytes != expected as u64 {
            return Err(Error::new(
                "INVALID_IMAGE",
                "BGRA byte length does not match dimensions",
            ));
        }
        Ok(expected)
    }
}

pub fn validate_frame(params: &Map<String, Value>, payload_bytes: u64) -> Result<usize, Error> {
    let frame: Frame = serde_json::from_value(Value::Object(params.clone()))
        .map_err(|_| Error::new("INVALID_PARAMS", "Invalid OCR frame parameters"))?;
    frame.validate(payload_bytes)
}

impl OcrService {
    pub async fn dispatch(
        &self,
        method: &str,
        params: &Map<String, Value>,
        bytes: Vec<u8>,
    ) -> Result<Value, Error> {
        if self.unusable.load(Ordering::Acquire) {
            return Err(unusable_error());
        }
        match method {
            "ocrInitialize" => {
                let options: Initialize = serde_json::from_value(Value::Object(params.clone()))
                    .map_err(|_| Error::new("INVALID_PARAMS", "Invalid OCR language parameters"))?;
                validate_language(options.engine, options.language.as_deref())?;
                self.bounded_worker(Duration::from_secs(5), move |worker| {
                    if options.engine == Engine::PpOcrV6 {
                        worker.pp_ocr()?;
                        return Ok(serde_json::json!({"engine": Engine::PpOcrV6, "resolvedLanguage": null}));
                    }
                    let result = worker
                        .windows(options.language.as_deref())?
                        .language()
                        .map_err(map_error);
                    if result.is_err() {
                        worker.windows = None;
                    }
                    result.map(|language| serde_json::json!({"resolvedLanguage": language}))
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
                self.bounded_worker(Duration::from_secs(5), |_| {
                    NativeOcr::available_languages()
                        .map(|languages| serde_json::json!({"languages": languages}))
                        .map_err(map_error)
                })
                .await
            }
            "ocrRecognizeBgra" => {
                let frame: Frame = serde_json::from_value(Value::Object(params.clone()))
                    .map_err(|_| Error::new("INVALID_PARAMS", "Invalid OCR frame parameters"))?;
                frame.validate(bytes.len() as u64)?;
                self.bounded_worker(
                    Duration::from_millis(frame.timeout_ms + 1000),
                    move |worker| {
                        if frame.engine == Engine::PpOcrV6 {
                            let result = worker
                                .pp_ocr()?
                                .recognize_bgra(&bytes, frame.width, frame.height)
                                .map_err(map_error)?;
                            return serde_json::to_value(result).map_err(|_| {
                                Error::new("OCR_ERROR", "OCR result serialization failed")
                            });
                        }
                        let engine = worker.windows(frame.language.as_deref())?;
                        let result = engine.recognize_bgra(
                            &bytes,
                            frame.width,
                            frame.height,
                            Duration::from_millis(frame.timeout_ms),
                        );
                        let poisoned = engine.is_poisoned();
                        if poisoned {
                            worker.windows = None;
                        }
                        match result {
                            Ok(result) => serde_json::to_value(result).map_err(|_| {
                                Error::new("OCR_ERROR", "OCR result serialization failed")
                            }),
                            Err(error) if poisoned && !matches!(error, OcrError::Timeout) => {
                                Err(Error::new("OCR_PROCESS_UNUSABLE", &error.to_string()))
                            }
                            Err(error) => Err(map_error(error)),
                        }
                    },
                )
                .await
            }
            _ => Err(Error::new("UNKNOWN_METHOD", "Unknown OCR method")),
        }
    }

    async fn bounded_worker(
        &self,
        timeout: Duration,
        work: impl FnOnce(&mut Worker) -> Result<Value, Error> + Send + 'static,
    ) -> Result<Value, Error> {
        let worker = Arc::clone(&self.worker);
        let unusable = Arc::clone(&self.unusable);
        let task = tokio::task::spawn_blocking(move || {
            let mut worker = worker.lock().map_err(|_| unusable_error())?;
            if unusable.load(Ordering::Acquire) {
                worker.clear();
                return Err(unusable_error());
            }
            let result = work(&mut worker);
            if result.as_ref().is_err_and(|error| {
                matches!(error.code.as_str(), "OCR_TIMEOUT" | "OCR_PROCESS_UNUSABLE")
            }) {
                unusable.store(true, Ordering::Release);
            }
            // A timed-out native call may finish after its caller has left.
            // It must never restore a usable engine to the session.
            if unusable.load(Ordering::Acquire) {
                worker.clear();
            }
            result
        });
        match tokio::time::timeout(timeout, task).await {
            Ok(Ok(result)) => result,
            Ok(Err(_)) => {
                self.unusable.store(true, Ordering::Release);
                Err(unusable_error())
            }
            Err(_) => {
                self.unusable.store(true, Ordering::Release);
                Err(map_error(OcrError::Timeout))
            }
        }
    }
}

fn map_error(error: OcrError) -> Error {
    let code = match error {
        OcrError::UnsupportedPlatform => "OCR_UNSUPPORTED_PLATFORM",
        OcrError::Init(_) => "OCR_INIT_FAILED",
        OcrError::Language(_) => "OCR_LANGUAGE_UNAVAILABLE",
        OcrError::Frame(_) => "INVALID_IMAGE",
        OcrError::Recognition(_) => "OCR_ERROR",
        OcrError::Timeout => "OCR_TIMEOUT",
    };
    Error::new(code, &error.to_string())
}

#[cfg(test)]
#[path = "ocr_service_tests.rs"]
mod tests;
