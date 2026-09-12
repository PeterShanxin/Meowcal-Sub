use super::config::{dedupe_paths, resolve_executable, validate_paths, validate_reviewed_resource};
use super::request::acquire_slot;
use super::*;
use std::path::Path;

#[path = "../build_support/core_version.rs"]
mod core_version;

#[test]
fn readiness_deadline_covers_server_budget_and_handshake() {
    assert!(READY_TIMEOUT >= meowcal_core::protocol::READY_BUDGET + HELLO_TIMEOUT);
}

#[test]
fn managed_backend_snapshots_do_not_wait_for_an_active_core_request() {
    use crate::llm::{FoundryLocalBackend, ReadyState, TranslatorBackend};
    let status = CoreStatus {
        installed: true,
        ready: true,
        model: "test".into(),
        version: CORE_VERSION.into(),
        storage_root: PathBuf::new(),
        managed_config: None,
        install_paths: None,
    };
    *STATUS.lock().unwrap() = Some(status);
    let config = crate::config::FoundryLocalConfig {
        managed_runtime: Some(crate::config::ManagedLocalRuntimeConfig {
            kind: "hy-mt".into(),
            executable_path: "unused".into(),
            model_path: "unused".into(),
            port: 0,
        }),
        ..Default::default()
    };
    let backend = FoundryLocalBackend::new(config);
    let active = TRANSLATION.get_or_init(|| Mutex::new(None)).lock().unwrap();
    let (send, receive) = std::sync::mpsc::channel();
    let reader = std::thread::spawn(move || {
        let _ = send.send((backend.is_available(), backend.ready_state()));
    });
    let result = receive.recv_timeout(Duration::from_millis(200));
    drop(active);
    reader.join().unwrap();
    assert_eq!(result.unwrap(), (true, ReadyState::Ready));
    invalidate_readiness();
    assert!(!cached_status().unwrap().ready);
    *STATUS.lock().unwrap() = None;
}

#[test]
fn production_path_uses_the_bundled_resource() {
    let path = resolve_executable("production", Path::new(r"C:\app"))
        .expect("production path should resolve");
    assert_eq!(
        path,
        PathBuf::from(r"C:\app\resources\core\meowcal-core.exe")
    );
}

#[test]
fn development_path_matches_the_compiled_runtime_selection() {
    if std::env::var_os("MEOWCAL_CORE_EXECUTABLE").is_some() {
        return;
    }
    let manifest_root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let path = resolve_executable("development", manifest_root)
        .expect("headless development path should resolve");
    assert!(path.is_absolute());
    if CORE_SOURCE_CANDIDATE {
        assert!(path.ends_with(Path::new("release/meowcal-core.exe")));
        assert!(path.is_absolute());
    } else {
        assert_eq!(path, manifest_root.join("resources/core/meowcal-core.exe"));
    }
}

#[test]
fn compiled_core_contract_has_a_supported_api_and_semantic_version() {
    assert_eq!(API_VERSION, 1);
    assert_eq!(CORE_VERSION.split('.').count(), 3);
    assert!(CORE_VERSION
        .split('.')
        .all(|segment| !segment.is_empty() && segment.bytes().all(|byte| byte.is_ascii_digit())));
}

#[test]
fn release_pin_handshake_requires_its_exact_version() {
    let hello = HelloResult {
        version: "0.1.1".into(),
        api: API_VERSION,
        capabilities: meowcal_core::protocol::CAPABILITIES
            .iter()
            .map(|capability| (*capability).to_string())
            .collect(),
    };
    assert!(hello_is_compatible(&hello, "0.1.1"));
    assert!(!hello_is_compatible(&hello, "0.1.0"));
}

#[test]
fn reviewed_runtime_requires_matching_metadata_and_digests() {
    let root =
        std::env::temp_dir().join(format!("meowcal-reviewed-resource-{}", std::process::id()));
    std::fs::create_dir_all(&root).expect("create reviewed resource directory");
    let executable = root.join("meowcal-core.exe");
    let license = root.join("LICENSE");
    std::fs::write(&executable, b"reviewed core").expect("write executable");
    std::fs::write(&license, b"license").expect("write license");
    let metadata = serde_json::json!({
        "schemaVersion": 1,
        "coreVersion": CORE_VERSION,
        "apiVersion": API_VERSION,
        "os": "windows",
        "architecture": if cfg!(target_arch = "aarch64") { "arm64" } else { "x64" },
        "executable": "meowcal-core.exe",
        "executableSha256": super::config::sha256_file(&executable).expect("hash executable"),
        "license": "LICENSE",
        "licenseSha256": super::config::sha256_file(&license).expect("hash license")
    });
    std::fs::write(root.join("meowcal-core.json"), format!("{metadata}\n"))
        .expect("write metadata");
    let executable_hash = metadata["executableSha256"]
        .as_str()
        .expect("trusted executable hash");
    let license_hash = metadata["licenseSha256"]
        .as_str()
        .expect("trusted license hash");
    validate_reviewed_resource(&executable, executable_hash, license_hash)
        .expect("reviewed resource is valid");
    std::fs::write(&executable, b"unreviewed same-version core").expect("replace executable");
    let mut replacement_metadata = metadata.clone();
    replacement_metadata["executableSha256"] = super::config::sha256_file(&executable)
        .expect("hash replacement")
        .into();
    std::fs::write(
        root.join("meowcal-core.json"),
        format!("{replacement_metadata}\n"),
    )
    .expect("replace adjacent metadata");
    assert!(validate_reviewed_resource(&executable, executable_hash, license_hash).is_err());
    std::fs::remove_file(root.join("meowcal-core.json")).expect("remove metadata");
    assert!(
        validate_reviewed_resource(&executable, executable_hash, license_hash)
            .expect_err("missing metadata must reject the reviewed pin")
            .starts_with("CORE_RELEASE_METADATA_MISSING")
    );
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn storage_paths_must_be_absolute_and_are_deduplicated() {
    assert!(validate_paths(Some(&PathBuf::from("relative")), &[]).is_err());
    let root = PathBuf::from(r"C:\models");
    assert_eq!(dedupe_paths(vec![root.clone(), root.clone()]), vec![root]);
}

#[test]
fn timeout_errors_that_exit_core_are_fatal() {
    assert!(fatal_remote("OCR_TIMEOUT"));
    assert!(fatal_remote("OCR_PROCESS_UNUSABLE"));
    assert!(fatal_remote("INSTALL_TIMEOUT"));
    assert!(fatal_remote("READY_TIMEOUT"));
    assert!(!fatal_remote("ASSETS_UNVERIFIED"));
}

#[test]
fn waiting_status_timeout_does_not_interrupt_the_active_request() {
    let slot = Arc::new(Mutex::new(None));
    let active_completed = Arc::new(AtomicBool::new(false));
    let owner_slot = slot.clone();
    let owner_completed = active_completed.clone();
    let (locked, wait_until_locked) = std::sync::mpsc::channel();
    let owner = std::thread::spawn(move || {
        let _guard = owner_slot.lock().expect("active request owns its slot");
        locked.send(()).expect("signal acquired slot");
        std::thread::sleep(Duration::from_millis(150));
        owner_completed.store(true, Ordering::SeqCst);
    });
    wait_until_locked
        .recv()
        .expect("active request acquired slot");

    let error = match acquire_slot(
        &slot,
        Instant::now() + Duration::from_millis(30),
        "status",
        None,
    ) {
        Ok(_) => panic!("status should time out behind the active request"),
        Err(error) => error,
    };

    assert_eq!(error, "CORE_REQUEST_TIMEOUT: status");
    assert!(!active_completed.load(Ordering::SeqCst));
    owner
        .join()
        .expect("active request should continue normally");
    assert!(active_completed.load(Ordering::SeqCst));
}

#[tokio::test]
#[ignore = "requires MEOWCAL_CORE_EXECUTABLE pointing to a built Core"]
async fn real_core_handshake_status_and_shutdown() {
    let storage = std::env::temp_dir().join(format!(
        "meowcal-sub-core-contract-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system clock")
            .as_nanos()
    ));
    register_headless(Some(storage.clone()), Vec::new()).expect("register real Core");

    let status = status().await.expect("Core handshake and status");
    assert_eq!(status.version, CORE_VERSION);
    assert!(!status.ready);
    assert!(owned_pid().is_some());

    shutdown_owned();
    assert!(owned_pid().is_none());
    let _ = std::fs::remove_dir_all(storage);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires MEOWCAL_CORE_EXECUTABLE and MEOWCAL_CORE_TEST_STORAGE with an installed model"]
async fn real_core_active_completion_cancel_preserves_warm_process() {
    let storage = PathBuf::from(
        std::env::var_os("MEOWCAL_CORE_TEST_STORAGE")
            .expect("MEOWCAL_CORE_TEST_STORAGE must name the installed storage base"),
    );
    register_headless(Some(storage), Vec::new()).expect("register real Core");
    let _shutdown = scopeguard::guard((), |_| shutdown_owned());
    let status = ready(READY_TIMEOUT).await.expect("ready Core");
    assert!(status.ready);
    let pid = owned_pid().expect("owned Core PID");

    let first = tokio::spawn(complete(completion_request(&status.model), 90_000));
    tokio::time::sleep(Duration::from_millis(250)).await;
    assert!(!first.is_finished(), "first completion must be active");
    first.abort();
    assert!(first.await.expect_err("caller cancellation").is_cancelled());

    let response = complete(completion_request(&status.model), 90_000)
        .await
        .expect("next completion should reuse the warm Core");
    assert_eq!(owned_pid(), Some(pid));
    let text = response
        .pointer("/choices/0/message/content")
        .and_then(Value::as_str)
        .expect("OpenAI completion text")
        .to_ascii_lowercase();
    assert!(text.contains("clock") && text.contains("tower"));
    shutdown_owned();
    assert!(owned_pid().is_none());
}

fn completion_request(model: &str) -> Value {
    json!({
        "model": model,
        "messages": [{
            "role": "user",
            "content": "Translate the following segment into English, without additional explanation.\n\n先不提时钟塔"
        }],
        "temperature": 0.7,
        "top_k": 20,
        "top_p": 0.6,
        "repeat_penalty": 1.05,
        "max_tokens": 120,
        "stream": false
    })
}

#[test]
fn binary_ocr_validates_dimensions_stride_size_and_timeout() {
    let valid = OcrRecognizeParams {
        language: None,
        width: 1,
        height: 1,
        stride: 4,
        timeout_ms: 30_000,
    };
    assert!(Request::ocr(valid.clone(), vec![0; 4]).is_ok());
    assert!(Request::ocr(valid.clone(), vec![0; 3]).is_err());
    for params in [
        OcrRecognizeParams {
            width: 0,
            ..valid.clone()
        },
        OcrRecognizeParams {
            width: 4097,
            ..valid.clone()
        },
        OcrRecognizeParams {
            height: 4097,
            ..valid.clone()
        },
        OcrRecognizeParams {
            stride: 8,
            ..valid.clone()
        },
        OcrRecognizeParams {
            timeout_ms: 0,
            ..valid.clone()
        },
        OcrRecognizeParams {
            timeout_ms: 30_001,
            ..valid.clone()
        },
    ] {
        assert!(Request::ocr(params, vec![0; 4]).is_err());
    }
    assert_eq!(meowcal_core::ocr::MAX_FRAME_BYTES, 64 * 1024 * 1024);
}
