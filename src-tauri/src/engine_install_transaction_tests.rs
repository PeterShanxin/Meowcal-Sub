use super::*;
use std::time::{SystemTime, UNIX_EPOCH};

#[tokio::test]
async fn multi_asset_failure_can_restore_previous_install() {
    let root = fixture_root("rollback");
    fs::create_dir_all(&root).await.unwrap();
    let final_a = root.join("runtime");
    let final_b = root.join("model");
    let candidate_a = root.join("runtime.candidate");
    let candidate_b = root.join("model.candidate");
    fs::write(&final_a, b"old-runtime").await.unwrap();
    fs::write(&final_b, b"old-model").await.unwrap();
    fs::write(&candidate_a, b"new-runtime").await.unwrap();
    fs::write(&candidate_b, b"new-model").await.unwrap();

    let promotion = promote_assets(&[
        (candidate_a, final_a.clone()),
        (candidate_b, final_b.clone()),
    ])
    .await
    .unwrap();
    promotion.rollback().await;

    assert_eq!(fs::read(final_a).await.unwrap(), b"old-runtime");
    assert_eq!(fs::read(final_b).await.unwrap(), b"old-model");
    fs::remove_dir_all(root).await.unwrap();
}

#[tokio::test]
async fn interrupted_promotion_restores_backup() {
    let root = fixture_root("interrupted");
    fs::create_dir_all(&root).await.unwrap();
    let final_path = root.join("engine");
    let backup = backup_path(&final_path);
    fs::write(&backup, b"known-good").await.unwrap();

    recover_interrupted_promotion(&final_path).await.unwrap();

    assert_eq!(fs::read(final_path).await.unwrap(), b"known-good");
    assert!(!backup.exists());
    fs::remove_dir_all(root).await.unwrap();
}

#[tokio::test]
async fn interrupted_uncommitted_candidate_never_replaces_backup() {
    let root = fixture_root("interrupted-candidate");
    fs::create_dir_all(&root).await.unwrap();
    let final_path = root.join("engine");
    let backup = backup_path(&final_path);
    fs::write(&final_path, b"uncommitted").await.unwrap();
    fs::write(&backup, b"known-good").await.unwrap();

    recover_interrupted_promotion(&final_path).await.unwrap();

    assert_eq!(fs::read(final_path).await.unwrap(), b"known-good");
    assert!(!backup.exists());
    fs::remove_dir_all(root).await.unwrap();
}

#[tokio::test]
async fn committed_promotion_removes_rollback_assets() {
    let root = fixture_root("commit");
    fs::create_dir_all(&root).await.unwrap();
    let final_path = root.join("engine");
    let candidate = root.join("engine.candidate");
    fs::write(&final_path, b"old").await.unwrap();
    fs::write(&candidate, b"new").await.unwrap();

    promote_assets(&[(candidate, final_path.clone())])
        .await
        .unwrap()
        .commit()
        .await
        .unwrap();

    assert_eq!(fs::read(&final_path).await.unwrap(), b"new");
    assert!(!backup_path(&final_path).exists());
    fs::remove_dir_all(root).await.unwrap();
}

#[tokio::test]
async fn corrupt_primary_state_recovers_valid_backup() {
    let root = fixture_root("state-backup");
    fs::create_dir_all(&root).await.unwrap();
    let state = InstallState {
        schema_version: STATE_SCHEMA_VERSION,
        ..InstallState::default()
    };
    fs::write(root.join(STATE_FILE), b"{broken").await.unwrap();
    fs::write(
        backup_path(&root.join(STATE_FILE)),
        serde_json::to_vec(&state).unwrap(),
    )
    .await
    .unwrap();

    assert_eq!(
        load_state(&root).await.unwrap().schema_version,
        STATE_SCHEMA_VERSION
    );
    fs::remove_dir_all(root).await.unwrap();
}

#[tokio::test]
async fn corrupt_active_engine_falls_back_to_last_known_good() {
    let root = fixture_root("last-known-good");
    fs::create_dir_all(&root).await.unwrap();
    let first = installed_record(&root, "v1", b"runtime-v1", b"model-v1").await;
    let second = installed_record(&root, "v2", b"runtime-v2", b"model-v2").await;
    record_active(&root, first.clone()).await.unwrap();
    record_active(&root, second.clone()).await.unwrap();
    fs::write(root.join(&second.model), b"corrupt")
        .await
        .unwrap();

    let recovered = recover_active(&root).await.unwrap();

    assert_eq!(recovered.executable, root.join(first.executable));
    assert_eq!(recovered.model, root.join(first.model));
    fs::remove_dir_all(root).await.unwrap();
}

#[test]
fn state_paths_cannot_escape_engine_root() {
    assert!(validate_relative(Path::new(r"..\outside.exe")).is_err());
    assert!(validate_relative(Path::new(r"runtime\engine.exe")).is_ok());
}

fn fixture_root(label: &str) -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!(
        "meowcal-engine-{label}-{}-{unique}",
        std::process::id()
    ))
}

async fn installed_record(
    root: &Path,
    version: &str,
    executable: &[u8],
    model: &[u8],
) -> InstalledEngine {
    let runtime_dir = PathBuf::from("runtime").join(version);
    let model_dir = PathBuf::from("models").join(version);
    let executable_path = runtime_dir.join("server.exe");
    let model_path = model_dir.join("model.gguf");
    fs::create_dir_all(root.join(&runtime_dir)).await.unwrap();
    fs::create_dir_all(root.join(&model_dir)).await.unwrap();
    fs::write(root.join(&executable_path), executable)
        .await
        .unwrap();
    fs::write(root.join(&model_path), model).await.unwrap();
    InstalledEngine {
        engine_version: version.to_string(),
        runtime_id: format!("runtime-{version}"),
        architecture: "x86_64".to_string(),
        runtime_dir,
        runtime_archive: PathBuf::from("runtime").join(format!("{version}.zip")),
        executable: executable_path,
        executable_size: executable.len() as u64,
        executable_sha256: sha256_file(&root.join("runtime").join(version).join("server.exe"))
            .unwrap(),
        model_dir,
        model: model_path,
        model_size: model.len() as u64,
        model_sha256: sha256_file(&root.join("models").join(version).join("model.gguf")).unwrap(),
    }
}
