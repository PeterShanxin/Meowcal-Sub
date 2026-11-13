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
fn interface_language_survives_durable_save_and_backup() {
    use crate::ui_language::UiLanguage;
    let dir = temp_dir("ui-language");
    let path = dir.join("config.json");
    let config = AppConfig {
        ui_language: UiLanguage::SimplifiedChinese,
        source_language: "ja-JP".into(),
        target_language: "en-US".into(),
        ..AppConfig::default()
    };
    save_to(&path, &config, Provenance::Authoritative).unwrap();
    let LoadOutcome::Loaded(saved) = read_from(&path) else {
        panic!("saved config should load");
    };
    assert_eq!(saved.ui_language, UiLanguage::SimplifiedChinese);
    let LoadOutcome::Loaded(backup) = read_from(&backup_path(&path)) else {
        panic!("backup should retain the selected language");
    };
    assert_eq!(backup.ui_language, UiLanguage::SimplifiedChinese);
    assert_eq!(backup.source_language, "ja-JP");
    assert_eq!(backup.target_language, "en-US");
}

#[test]
fn failed_save_does_not_publish_an_interface_language_change() {
    use crate::ui_language::UiLanguage;
    let live = std::sync::Mutex::new(AppConfig::default());
    let result = update_live(
        &live,
        |config| config.ui_language = UiLanguage::SimplifiedChinese,
        |_| Err("disk unavailable".into()),
    );
    assert!(result.is_err());
    assert_eq!(live.lock().unwrap().ui_language, UiLanguage::System);
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

#[test]
fn concurrent_live_region_and_settings_saves_survive_restart() {
    for region_first in [true, false] {
        assert_concurrent_live_saves(region_first);
    }
}

fn assert_concurrent_live_saves(region_first: bool) {
    use std::sync::{mpsc, Arc, Mutex};
    use std::time::Duration;

    let dir = temp_dir(&format!("concurrent-live-region-settings-{region_first}"));
    let path = dir.join("config.json");
    let live = Arc::new(Mutex::new(AppConfig::default()));
    let region = crate::config::CaptureRegion::new(50, 60, 640, 100);
    let monitor = crate::capture_target::CaptureMonitor {
        device_id: "test-monitor".into(),
        bounds: crate::config::CaptureRegion::new(-2560, 0, 2560, 1440),
        scale_factor: 1.25,
    };
    let (writing, is_writing) = mpsc::channel();
    let (release, released) = mpsc::channel();
    let first_live = live.clone();
    let first_path = path.clone();
    let first_monitor = monitor.clone();
    let first = std::thread::spawn(move || {
        update_live(
            &first_live,
            |config| {
                if region_first {
                    config.last_capture_region = Some(region);
                    config.last_capture_scale_factor = Some(1.25);
                    config.last_capture_monitor = Some(first_monitor);
                } else {
                    config.target_language = "ja-JP".into();
                }
            },
            |config| {
                writing.send(()).unwrap();
                released.recv().unwrap();
                save_to(&first_path, config, Provenance::Authoritative)
            },
        )
        .unwrap();
    });
    is_writing.recv_timeout(Duration::from_secs(5)).unwrap();
    let (attempting, attempted) = mpsc::channel();
    let (finished, is_finished) = mpsc::channel();
    let second_live = live.clone();
    let second_path = path.clone();
    let second_monitor = monitor.clone();
    let second = std::thread::spawn(move || {
        attempting.send(()).unwrap();
        update_live(
            &second_live,
            |config| {
                if region_first {
                    config.target_language = "ja-JP".into();
                } else {
                    config.last_capture_region = Some(region);
                    config.last_capture_scale_factor = Some(1.25);
                    config.last_capture_monitor = Some(second_monitor);
                }
            },
            |config| save_to(&second_path, config, Provenance::Authoritative),
        )
        .unwrap();
        finished.send(()).unwrap();
    });
    attempted.recv_timeout(Duration::from_secs(5)).unwrap();
    // Hold the first write while a second writer tries to save. The assertion
    // is the persisted result, regardless of how the threads were scheduled.
    let _ = is_finished.recv_timeout(Duration::from_millis(100));
    release.send(()).unwrap();
    first.join().unwrap();
    second.join().unwrap();

    for config in [load_durable(&path).0, live.lock().unwrap().clone()] {
        assert_eq!(config.target_language, "ja-JP");
        assert_eq!(config.last_capture_region, Some(region));
        assert_eq!(config.last_capture_scale_factor, Some(1.25));
        assert_eq!(config.last_capture_monitor, Some(monitor.clone()));
    }
}

#[test]
fn a_failed_live_save_does_not_publish_unpersisted_settings() {
    let live = std::sync::Mutex::new(registered());
    let original = serde_json::to_value(live.lock().unwrap().clone()).unwrap();
    let error = update_live(
        &live,
        |config| config.target_language = "ja-JP".into(),
        |_| Err("disk unavailable".into()),
    );
    assert_eq!(error.unwrap_err(), "disk unavailable");
    assert_eq!(
        serde_json::to_value(live.lock().unwrap().clone()).unwrap(),
        original
    );
}
