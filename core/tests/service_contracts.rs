use meowcal_core::completion::{validate, CompletionParams};
use meowcal_core::protocol::{self, Request};
use meowcal_core::service::Service;
use meowcal_core::storage::{resolve_root, Lease};
use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::Duration;

fn temporary_root(label: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "meowcal-core-{label}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ))
}

fn request(method: &str, params: Value) -> Request {
    protocol::decode(
        &serde_json::to_vec(&json!({"id":1,"api":1,"method":method,"params":params})).unwrap(),
    )
    .unwrap()
}

#[test]
fn protocol_requires_envelope_and_matching_api() {
    assert_eq!(
        protocol::decode(br#"{"id":1,"api":2,"method":"status","params":{}}"#)
            .unwrap_err()
            .code,
        "API_MISMATCH"
    );
    assert!(protocol::decode(
        br#"{"id":1,"api":1,"method":"status","params":{},"endpoint":"http://remote"}"#
    )
    .is_err());
    assert!(protocol::decode(br#"{"id":1,"api":1,"method":"status","params":[]}"#).is_err());
    let oversized = json!({"id":1,"api":1,"method":"complete","params":{"text":"x".repeat(protocol::MAX_FRAME_BYTES)}});
    assert_eq!(
        protocol::decode(&serde_json::to_vec(&oversized).unwrap())
            .unwrap_err()
            .code,
        "FRAME_TOO_LARGE"
    );
}

#[test]
fn completion_fixes_model_transport_and_budgets() {
    let valid = json!({"model":"owned","messages":[{"role":"user","content":"private text"}],"max_tokens":150,"temperature":0.3,"top_k":20,"top_p":0.6,"repeat_penalty":1.05});
    assert!(validate(
        &CompletionParams {
            request: valid.clone(),
            timeout_ms: 1000
        },
        "owned"
    )
    .is_ok());
    for (key, value) in [
        ("model", json!("other")),
        ("endpoint", json!("http://remote")),
        ("max_tokens", json!(0)),
        ("max_tokens", json!(4097)),
        ("stream", json!(true)),
        ("temperature", json!(3.0)),
        ("top_k", json!(-1)),
    ] {
        let mut bad = valid.clone();
        bad[key] = value;
        assert!(
            validate(
                &CompletionParams {
                    request: bad,
                    timeout_ms: 1000
                },
                "owned"
            )
            .is_err(),
            "{key}"
        );
    }
    assert!(validate(
        &CompletionParams {
            request: valid,
            timeout_ms: 90_001
        },
        "owned"
    )
    .is_err());
}

#[tokio::test]
async fn initialization_is_versioned_and_status_never_installs_or_starts() {
    let base = temporary_root("status");
    let mut service = Service::default();
    assert_eq!(
        service
            .handle(request("status", json!({})), &|_| {})
            .await
            .unwrap_err()
            .code,
        "NOT_INITIALIZED"
    );
    let hello = json!({"client":"sub1","profile":"development","expectedVersion":"0.0.9","storageRoot":base});
    assert_eq!(
        service
            .handle(request("hello", hello.clone()), &|_| {})
            .await
            .unwrap_err()
            .code,
        "VERSION_MISMATCH"
    );
    let mut hello = hello;
    hello["expectedVersion"] = json!("0.1.0");
    let result = service
        .handle(request("hello", hello), &|_| {})
        .await
        .unwrap();
    assert_eq!(result["api"], 1);
    assert!(result["capabilities"]
        .as_array()
        .unwrap()
        .contains(&json!("ocrRecognize")));
    let status = service
        .handle(request("status", json!({})), &|_| {})
        .await
        .unwrap();
    assert_eq!(status["installed"], false);
    assert_eq!(status["ready"], false);
    assert!(!base.exists());
    assert!(service
        .handle(request("ready", json!({})), &|_| {})
        .await
        .is_err());
    assert!(meowcal_core::hy_mt_runtime::owned_pid().is_none());
    let root = resolve_root("development", Some(&base)).unwrap();
    assert!(!root.join("runtime").exists());
    drop(service);
    std::fs::remove_dir_all(base).unwrap();
}

#[tokio::test]
async fn shared_runtime_leases_exclude_installation() {
    let root = temporary_root("lease");
    let first = Lease::acquire(&root, false, Duration::ZERO).await.unwrap();
    let second = Lease::acquire(&root, false, Duration::ZERO).await.unwrap();
    assert!(Lease::acquire(&root, true, Duration::from_millis(20))
        .await
        .is_err());
    drop(first);
    drop(second);
    let exclusive = Lease::acquire(&root, true, Duration::ZERO).await.unwrap();
    assert!(Lease::acquire(&root, false, Duration::ZERO).await.is_err());
    drop(exclusive);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn process_negotiates_and_exits_after_shutdown() {
    let mut child = Command::new(env!("CARGO_BIN_EXE_meowcal-core"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut input = child.stdin.take().unwrap();
    let mut output = BufReader::new(child.stdout.take().unwrap());
    let root = temporary_root("process");
    for (id, method, params) in [
        (
            1,
            "hello",
            json!({"client":"sub2","profile":"production","expectedVersion":"0.1.0","storageRoot":root}),
        ),
        (2, "status", json!({})),
        (3, "shutdown", json!({})),
    ] {
        writeln!(
            input,
            "{}",
            json!({"id":id,"api":1,"method":method,"params":params})
        )
        .unwrap();
        input.flush().unwrap();
        let mut line = String::new();
        output.read_line(&mut line).unwrap();
        let value: Value = serde_json::from_str(&line).unwrap();
        assert_eq!(value["id"], id);
        assert!(value.get("error").is_none(), "{value}");
    }
    assert!(child.wait().unwrap().success());
    assert!(!root.exists());
}

#[test]
fn storage_profiles_and_versions_never_alias() {
    let base = temporary_root("paths");
    let prod = resolve_root("production", Some(&base)).unwrap();
    let dev = resolve_root("development", Some(&base)).unwrap();
    assert_ne!(prod, dev);
    assert!(prod.starts_with(base.join("production").join("0.1.0")));
    assert!(resolve_root("other", Some(&base)).is_err());
    assert!(resolve_root("production", Some(std::path::Path::new("relative"))).is_err());
}

#[cfg(windows)]
#[tokio::test]
async fn offline_import_checks_disk_before_copy_and_never_adopts_dll_tree() {
    use meowcal_core::engine_manifest::EngineManifest;
    use meowcal_core::hy_mt_runtime::HyMtInstallPaths;
    use meowcal_core::storage::import_legacy;
    use sha2::{Digest, Sha256};
    let base = temporary_root("migration");
    let mut manifest = EngineManifest::shipped().unwrap();
    for runtime in &mut manifest.runtimes {
        runtime.archive.size_bytes = 7;
        runtime.archive.sha256 = format!("{:x}", Sha256::digest(b"archive"));
    }
    manifest.model.artifact.size_bytes = 5;
    manifest.model.artifact.sha256 = format!("{:x}", Sha256::digest(b"model"));
    manifest.requirements.minimum_windows_build = 0;
    manifest.requirements.minimum_ram_bytes = 0;
    manifest.requirements.minimum_free_disk_bytes = u64::MAX;
    let runtime = manifest.runtime_for_current_arch().unwrap();
    let source = HyMtInstallPaths::from_cache_root(base.join("legacy"), &manifest, runtime);
    let target = HyMtInstallPaths::from_cache_root(base.join("core"), &manifest, runtime);
    std::fs::create_dir_all(source.runtime_archive.parent().unwrap()).unwrap();
    std::fs::create_dir_all(&source.model_dir).unwrap();
    std::fs::create_dir_all(&source.runtime_dir).unwrap();
    std::fs::write(&source.runtime_archive, b"archive").unwrap();
    std::fs::write(&source.model, b"model").unwrap();
    std::fs::write(&source.executable, b"untrusted executable").unwrap();
    assert!(
        import_legacy(&target, std::slice::from_ref(&source.root), &manifest, true)
            .await
            .is_err()
    );
    assert!(!target.model.exists());
    assert!(!target.runtime_archive.exists());
    manifest.requirements.minimum_free_disk_bytes = 0;
    import_legacy(&target, std::slice::from_ref(&source.root), &manifest, true)
        .await
        .unwrap();
    assert_eq!(std::fs::read(&target.model).unwrap(), b"model");
    assert_eq!(std::fs::read(&target.runtime_archive).unwrap(), b"archive");
    assert!(!target.executable.exists());
    assert!(source.executable.exists());
    assert!(source.model.exists());
    std::fs::remove_file(&target.model).unwrap();
    std::fs::write(&source.model, b"wrong").unwrap();
    assert!(import_legacy(&target, &[source.root], &manifest, true)
        .await
        .unwrap_err()
        .starts_with("CORE_ASSETS_UNVERIFIED"));
    assert!(!target.model.exists());
    std::fs::remove_dir_all(base).unwrap();
}

#[tokio::test]
async fn eof_cancels_install_waiting_on_another_process_lease() {
    let base = temporary_root("disconnect");
    let root = resolve_root("development", Some(&base)).unwrap();
    let lease = Lease::acquire(&root, false, Duration::ZERO).await.unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_meowcal-core"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let mut input = child.stdin.take().unwrap();
    let mut output = BufReader::new(child.stdout.take().unwrap());
    writeln!(input, "{}", json!({"id":1,"api":1,"method":"hello","params":{"client":"sub1","profile":"development","expectedVersion":"0.1.0","storageRoot":base}})).unwrap();
    input.flush().unwrap();
    let mut line = String::new();
    output.read_line(&mut line).unwrap();
    assert!(serde_json::from_str::<Value>(&line)
        .unwrap()
        .get("result")
        .is_some());
    writeln!(
        input,
        "{}",
        json!({"id":2,"api":1,"method":"install","params":{}})
    )
    .unwrap();
    input.flush().unwrap();
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert!(child.try_wait().unwrap().is_none());
    drop(input);
    let deadline = std::time::Instant::now() + Duration::from_secs(3);
    loop {
        if let Some(status) = child.try_wait().unwrap() {
            assert!(status.success());
            break;
        }
        if std::time::Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            panic!("Core did not cancel installation on EOF");
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert!(!root.join("runtime").exists());
    drop(lease);
    let exclusive = Lease::acquire(&root, true, Duration::ZERO).await.unwrap();
    drop(exclusive);
    std::fs::remove_dir_all(base).unwrap();
}

#[tokio::test]
async fn interrupted_import_cleans_only_owned_staging_under_exclusive_lease() {
    use meowcal_core::engine_manifest::EngineManifest;
    use meowcal_core::hy_mt_runtime::HyMtInstallPaths;
    let root = temporary_root("staging");
    let manifest = EngineManifest::shipped().unwrap();
    let paths = HyMtInstallPaths::from_cache_root(
        &root,
        &manifest,
        manifest.runtime_for_current_arch().unwrap(),
    );
    let lease = Lease::acquire(&root, true, Duration::ZERO).await.unwrap();
    let mut staging = vec![
        paths.runtime_archive.with_extension("import-part"),
        paths.model.with_extension("import-part"),
    ];
    for asset in [&paths.runtime_dir, &paths.model] {
        let mut name = asset.as_os_str().to_owned();
        name.push(".candidate");
        staging.push(PathBuf::from(name));
    }
    for path in &staging {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, b"interrupted").unwrap();
    }
    std::fs::remove_file(&staging[2]).unwrap();
    std::fs::create_dir_all(&staging[2]).unwrap();
    std::fs::write(staging[2].join("stale.dll"), b"interrupted").unwrap();
    let unrelated = root.join("keep.txt");
    let resumable = paths.runtime_archive.with_extension("download.part");
    std::fs::write(&unrelated, b"keep").unwrap();
    std::fs::write(&resumable, b"resume").unwrap();
    let error = meowcal_core::storage::import_legacy(&paths, &[], &manifest, true)
        .await
        .unwrap_err();
    assert!(error.starts_with("CORE_ASSETS_UNVERIFIED"));
    assert!(staging.iter().all(|path| !path.exists()));
    assert_eq!(std::fs::read(unrelated).unwrap(), b"keep");
    assert_eq!(std::fs::read(resumable).unwrap(), b"resume");
    drop(lease);
    std::fs::remove_dir_all(root).unwrap();
}
