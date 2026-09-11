use super::*;

fn frame(width: u32, stride: u32) -> Frame {
    Frame {
        language: None,
        width,
        height: 1,
        stride,
        timeout_ms: 1000,
    }
}

#[test]
fn validates_frame_before_starting_native_work() {
    assert!(frame(1, 4).validate(4).is_ok());
    assert_eq!(frame(1, 4).validate(3).unwrap_err().code, "INVALID_IMAGE");
    assert_eq!(frame(1, 8).validate(8).unwrap_err().code, "INVALID_IMAGE");
    let mut invalid = frame(1, 4);
    invalid.timeout_ms = 0;
    assert_eq!(invalid.validate(4).unwrap_err().code, "INVALID_PARAMS");
}

#[test]
fn header_validation_accepts_maximum_and_rejects_length_overflow() {
    let params = serde_json::json!({"width":4096,"height":4096,"stride":16384,"timeoutMs":30000});
    let params = params.as_object().unwrap();
    assert_eq!(
        validate_frame(params, 64 * 1024 * 1024).unwrap(),
        crate::ocr::MAX_FRAME_BYTES
    );
    assert_eq!(
        validate_frame(params, u64::MAX).unwrap_err().code,
        "INVALID_IMAGE"
    );
}

#[tokio::test]
async fn initialization_recognition_and_language_changes_share_one_worker() {
    let service = OcrService::default();
    let initialized = service
        .dispatch(
            "ocrInitialize",
            serde_json::json!({"language":null}).as_object().unwrap(),
            Vec::new(),
        )
        .await
        .unwrap();
    assert!(service
        .worker
        .lock()
        .unwrap()
        .cached
        .as_ref()
        .unwrap()
        .requested_language
        .is_none());
    let parameters =
        serde_json::json!({"language":null,"width":100,"height":80,"stride":400,"timeoutMs":5000});
    let result = service
        .dispatch(
            "ocrRecognizeBgra",
            parameters.as_object().unwrap(),
            vec![255; 32000],
        )
        .await
        .unwrap();
    assert_eq!(result["frameWidth"], 100.0);
    assert!(result["text"].as_str().unwrap().trim().is_empty());
    {
        let worker = service.worker.lock().unwrap();
        let cached = worker.cached.as_ref().unwrap();
        assert!(cached.requested_language.is_none());
        assert_eq!(
            cached.engine.language().unwrap(),
            initialized["resolvedLanguage"].as_str().unwrap()
        );
    }
    let languages = NativeOcr::available_languages().unwrap();
    let language = languages
        .iter()
        .find(|tag| Some(tag.as_str()) != initialized["resolvedLanguage"].as_str())
        .unwrap_or(&languages[0])
        .clone();
    let explicit = serde_json::json!({"language":language});
    let switched = service
        .dispatch("ocrInitialize", explicit.as_object().unwrap(), Vec::new())
        .await
        .unwrap();
    assert_eq!(switched["resolvedLanguage"], language);
    assert_eq!(
        service
            .worker
            .lock()
            .unwrap()
            .cached
            .as_ref()
            .unwrap()
            .requested_language
            .as_deref(),
        Some(language.as_str())
    );
    let unavailable = service
        .dispatch(
            "ocrInitialize",
            serde_json::json!({"language":"zz-ZZ"}).as_object().unwrap(),
            Vec::new(),
        )
        .await
        .unwrap_err();
    assert_eq!(unavailable.code, "OCR_LANGUAGE_UNAVAILABLE");
    assert!(service.worker.lock().unwrap().cached.is_none());
    let mut parameters = parameters;
    parameters["language"] = serde_json::json!(language);
    assert!(service
        .dispatch(
            "ocrRecognizeBgra",
            parameters.as_object().unwrap(),
            vec![255; 32000]
        )
        .await
        .is_ok());
    assert_eq!(
        service
            .worker
            .lock()
            .unwrap()
            .cached
            .as_ref()
            .unwrap()
            .requested_language
            .as_deref(),
        Some(language.as_str())
    );
}

#[tokio::test]
async fn timed_out_worker_cannot_accept_another_operation_or_restore_cache() {
    let service = OcrService::default();
    let (entered, started) = tokio::sync::oneshot::channel();
    let (release, blocked) = std::sync::mpsc::channel();
    let operation = service.bounded_worker(Duration::from_millis(50), move |worker| {
        worker.engine(None)?;
        let _ = entered.send(());
        let _ = blocked.recv_timeout(Duration::from_secs(1));
        Ok(serde_json::json!({}))
    });
    let waiter = async {
        let _ = started.await;
        // Keep the worker occupied until the caller's deadline has elapsed.
        tokio::time::sleep(Duration::from_millis(100)).await;
        let _ = release.send(());
    };
    let (result, ()) = tokio::join!(operation, waiter);
    assert_eq!(result.unwrap_err().code, "OCR_TIMEOUT");
    assert_eq!(
        service
            .dispatch("ocrLanguages", &Map::new(), Vec::new())
            .await
            .unwrap_err()
            .code,
        "OCR_PROCESS_UNUSABLE"
    );
    let worker = Arc::clone(&service.worker);
    assert!(
        tokio::task::spawn_blocking(move || worker.lock().unwrap().cached.is_none())
            .await
            .unwrap()
    );
}

#[tokio::test]
async fn worker_panic_permanently_invalidates_the_session() {
    let service = OcrService::default();
    let error = service
        .bounded_worker(Duration::from_secs(1), |_| {
            panic!("simulated native worker failure")
        })
        .await
        .unwrap_err();
    assert_eq!(error.code, "OCR_PROCESS_UNUSABLE");
    assert_eq!(
        service
            .dispatch("ocrLanguages", &Map::new(), Vec::new())
            .await
            .unwrap_err()
            .code,
        "OCR_PROCESS_UNUSABLE"
    );
}
