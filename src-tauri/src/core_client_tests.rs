use super::config::{dedupe_paths, resolve_executable, validate_paths};
use super::*;
use std::path::Path;

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
fn development_path_uses_the_target_release_binary() {
    if std::env::var_os("MEOWCAL_CORE_EXECUTABLE").is_some() {
        return;
    }
    let path =
        resolve_executable("development", Path::new("")).expect("development path should resolve");
    assert!(path.ends_with(Path::new("release/meowcal-core.exe")));
    assert!(path.is_absolute());
}

#[test]
fn packaged_core_contract_is_pinned_to_api_one_version_0_1_0() {
    assert_eq!(API_VERSION, 1);
    assert_eq!(CORE_VERSION, "0.1.0");
    assert_eq!(CORE_VERSION, meowcal_core::protocol::CORE_VERSION);
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

#[test]
#[ignore = "requires MEOWCAL_CORE_EXECUTABLE pointing to a built Core"]
fn real_core_handshake_status_and_shutdown() {
    let storage = std::env::temp_dir().join(format!(
        "meowcal-sub-core-contract-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system clock")
            .as_nanos()
    ));
    register_headless(Some(storage.clone()), Vec::new()).expect("register real Core");

    let status = status_blocking().expect("Core handshake and status");
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
    let status = ready(Duration::from_secs(90)).await.expect("ready Core");
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
