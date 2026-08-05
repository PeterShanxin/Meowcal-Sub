use crate::config::AppConfig;
use crate::config_save::*;
use crate::config_store::{backup_path, load_durable, read_from, LoadOutcome};
use crate::engine_config::ManagedLocalRuntimeConfig;
use std::fs;
use std::path::PathBuf;

fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("meowcal-config-save-{name}"));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("temp dir");
    dir
}

fn registered() -> AppConfig {
    let mut config = AppConfig::default();
    config.translation.foundry_local.model = Some("HY-MT1.5-1.8B-Q4_K_M".to_string());
    config.translation.foundry_local.endpoint_url = Some("http://127.0.0.1:11436".to_string());
    config.translation.foundry_local.managed_runtime = Some(ManagedLocalRuntimeConfig {
        kind: "hy-mt".to_string(),
        executable_path: r"D:\engine\llama-server.exe".to_string(),
        model_path: r"D:\engine\HY-MT.gguf".to_string(),
        port: 11_436,
    });
    config
}

#[test]
fn saving_keeps_the_backup_current() {
    let dir = temp_dir("backup-current");
    let path = dir.join("config.json");
    write_atomic(&path, &AppConfig::default()).unwrap();
    load_durable(&path); // backup is now the default config

    let mut later = registered();
    later.target_language = "ja-JP".to_string();
    write_atomic(&path, &later).unwrap();
    refresh_backup(&path);

    let LoadOutcome::Loaded(backup) = read_from(&backup_path(&path)) else {
        panic!("the backup should load");
    };
    assert_eq!(backup.target_language, "ja-JP");
}

#[test]
fn refreshing_the_backup_leaves_no_staging_file() {
    let dir = temp_dir("backup-staging");
    let path = dir.join("config.json");
    write_atomic(&path, &registered()).unwrap();

    load_durable(&path);

    assert!(backup_path(&path).is_file());
    assert!(!sibling(&path, "config.bak.tmp.json").exists());
}

#[test]
fn concurrent_saves_do_not_share_a_staging_file() {
    let dir = temp_dir("staging-unique");
    let path = dir.join("config.json");

    let first = staging_path(&path);
    let second = staging_path(&path);

    assert_ne!(first, second);
}

#[test]
fn writing_leaves_no_staging_file_behind() {
    let dir = temp_dir("staging");
    let path = dir.join("config.json");
    write_atomic(&path, &registered()).unwrap();

    assert!(!sibling(&path, "config.tmp.json").exists());
}

#[test]
fn a_save_cannot_clear_a_runtime_that_is_still_on_disk() {
    let dir = temp_dir("preserve");
    let path = dir.join("config.json");
    write_atomic(&path, &registered()).unwrap();

    let defaulted = AppConfig::default();
    save_to(&path, &defaulted, Provenance::Authoritative).unwrap();
    let LoadOutcome::Loaded(defaulted) = read_from(&path) else {
        panic!("written config should load")
    };

    assert!(defaulted
        .translation
        .foundry_local
        .managed_runtime
        .is_some());
    assert_eq!(
        defaulted.translation.foundry_local.model.as_deref(),
        Some("HY-MT1.5-1.8B-Q4_K_M")
    );
}

#[test]
fn preserving_adds_nothing_when_disk_has_no_runtime() {
    let dir = temp_dir("preserve-empty");
    let path = dir.join("config.json");
    write_atomic(&path, &AppConfig::default()).unwrap();

    let incoming = AppConfig::default();
    save_to(&path, &incoming, Provenance::Authoritative).unwrap();
    let LoadOutcome::Loaded(incoming) = read_from(&path) else {
        panic!("written config should load")
    };

    assert!(incoming.translation.foundry_local.managed_runtime.is_none());
}

// The #69 review's first critical. A corrupt config plus a briefly locked backup
// leaves the backup holding the only real settings - and the next window close
// used to copy factory defaults straight over it, reproducing #64 with the
// machinery built to prevent it.
#[test]
fn a_fallback_session_never_refreshes_the_backup() {
    let dir = temp_dir("fallback-backup");
    let path = dir.join("config.json");
    write_atomic(&path, &registered()).unwrap();
    refresh_backup(&path); // the backup now holds the real settings
    fs::remove_file(&path).unwrap();

    // A fallback session running on defaults saves for any ordinary reason.
    save_to(&path, &AppConfig::default(), Provenance::Fallback).unwrap();

    let LoadOutcome::Loaded(backup) = read_from(&backup_path(&path)) else {
        panic!("the backup should still load");
    };
    assert!(
        backup.translation.foundry_local.managed_runtime.is_some(),
        "the backup must still hold the real settings, not the defaults just saved"
    );
}

// The second critical. A config that was only locked is served from the backup;
// once the lock releases, saving that stale copy over the real file destroys
// newer settings - and this path deliberately keeps no quarantine copy.
#[test]
fn a_fallback_session_does_not_overwrite_a_config_that_came_back() {
    let dir = temp_dir("fallback-readable");
    let path = dir.join("config.json");
    let mut real = registered();
    real.target_language = "ja-JP".to_string();
    write_atomic(&path, &real).unwrap();

    let refused = save_to(&path, &AppConfig::default(), Provenance::Fallback);

    assert!(
        refused.is_err(),
        "a fallback copy must not replace a readable config"
    );
    let LoadOutcome::Loaded(on_disk) = read_from(&path) else {
        panic!("the real config should still load");
    };
    assert_eq!(on_disk.target_language, "ja-JP");
}

// An authoritative session is the normal case and must still save freely.
#[test]
fn an_authoritative_session_saves_and_refreshes() {
    let dir = temp_dir("authoritative");
    let path = dir.join("config.json");
    write_atomic(&path, &AppConfig::default()).unwrap();

    let mut later = registered();
    later.target_language = "ko-KR".to_string();
    save_to(&path, &later, Provenance::Authoritative).unwrap();

    let LoadOutcome::Loaded(backup) = read_from(&backup_path(&path)) else {
        panic!("the backup should load");
    };
    assert_eq!(backup.target_language, "ko-KR");
}
