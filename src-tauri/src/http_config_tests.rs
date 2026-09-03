use crate::app_profile::AppProfile;
use crate::config::{AppConfig, CaptureRegion, WindowPreferences};
use crate::config_save::{remember_provenance, session_provenance, write_atomic, Provenance};
use crate::config_store::{backup_path, load_durable, read_from, LoadOutcome};
use crate::engine_config::ManagedLocalRuntimeConfig;
use crate::http_config::*;
use std::fs;
use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard, OnceLock};

use super::standalone_config_path_for;

/// Serialises the tests that save, because provenance is process-wide.
///
/// Rust runs tests in parallel by default, so a test that flips provenance and
/// one that saves expecting authority can otherwise overlap.
fn provenance_lock() -> MutexGuard<'static, ()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("meowcal-http-config-{name}"));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("temp dir");
    dir
}

fn registered() -> AppConfig {
    let mut config = AppConfig::default();
    config.translation.foundry_local.model = Some("HY-MT1.5-1.8B-Q4_K_M".to_string());
    config.translation.foundry_local.managed_runtime = Some(ManagedLocalRuntimeConfig {
        kind: "hy-mt".to_string(),
        executable_path: r"D:\engine\llama-server.exe".to_string(),
        model_path: r"D:\engine\HY-MT.gguf".to_string(),
        port: 11_436,
    });
    config
}

// The #64 failure, entered through the dev-mode door. Before #71 this loader
// was `read_to_string` -> `from_str` -> `default()`, so a truncated config came
// back as factory settings and the next save wrote them over the backup too.
#[test]
fn a_config_the_dev_server_cannot_parse_is_recovered_rather_than_reset() {
    // Loading records provenance, and a recovered load records `Fallback` -
    // which is the point of it. Held under the same lock as the saves so that
    // this test's fallback state cannot leak into one of theirs.
    let _guard = provenance_lock();
    let dir = temp_dir("corrupt-recovers");
    let path = dir.join("config.json");
    write_atomic(&path, &registered()).unwrap();
    load_durable(&path); // establishes the backup
    fs::write(&path, "{\"targetLanguage\": ").unwrap(); // interrupted write

    let loaded = load_standalone_config(&path);
    // Read and restored before asserting: a failure here would otherwise leave
    // the process-global at `Fallback` for whatever test ran next.
    let recorded = session_provenance();
    remember_provenance(Provenance::Authoritative);

    assert_eq!(
        recorded,
        Provenance::Fallback,
        "a recovered load must mark the session, or its saves will overwrite \
         a config that recovers underneath it"
    );
    assert_eq!(
        loaded
            .translation
            .foundry_local
            .managed_runtime
            .map(|runtime| runtime.port),
        Some(11_436),
        "the engine registration should survive a config the dev server cannot parse"
    );
}

// The guard that made #64 permanent rather than merely bad: a save that writes
// out an in-memory config with no runtime, over a disk config that has one.
#[test]
fn a_dev_mode_save_keeps_an_engine_registration_that_is_only_on_disk() {
    let _guard = provenance_lock();
    remember_provenance(Provenance::Authoritative);
    let dir = temp_dir("save-preserves-runtime");
    let path = dir.join("config.json");
    write_atomic(&path, &registered()).unwrap();

    let mut settings = AppConfig::default();
    settings.target_language = "ja-JP".to_string();
    save_standalone_config(&path, &settings).expect("save should succeed");

    let LoadOutcome::Loaded(on_disk) = read_from(&path) else {
        panic!("the saved config should load");
    };
    assert_eq!(on_disk.target_language, "ja-JP", "the edit should land");
    assert!(
        on_disk.translation.foundry_local.managed_runtime.is_some(),
        "the registration should not be written away by a save that never had it"
    );
}

// Checks only what it says: the save cleans up after itself. It does *not*
// demonstrate atomicity - a plain `fs::write` would pass it too, since a writer
// that stages nothing leaves no staging file. Atomicity is a property of
// `write_atomic`, and is pinned in `config_save_tests`; what is worth pinning
// here is that the dev door leaves no debris in the user's profile directory.
#[test]
fn a_dev_mode_save_leaves_no_staging_file_behind() {
    let _guard = provenance_lock();
    remember_provenance(Provenance::Authoritative);
    let dir = temp_dir("save-atomic");
    let path = dir.join("config.json");

    save_standalone_config(&path, &registered()).expect("save should succeed");

    assert!(matches!(read_from(&path), LoadOutcome::Loaded(_)));
    let strays: Vec<_> = fs::read_dir(&dir)
        .expect("dir")
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| name.contains(".tmp."))
        .collect();
    assert!(strays.is_empty(), "unexpected staging files: {strays:?}");
}

// A fallback session is holding the backup's contents, not the user's. If the
// real config is readable again it is newer, and the dev server must not write
// over it - the same rule the Tauri path follows, which only holds here because
// this save passes the session provenance rather than assuming authority.
//
// Provenance is process-wide, so this test sets it and puts it back. Asserting
// against whatever it happened to be instead made the test depend on the order
// the suite ran in, which is how it first failed.
#[test]
fn a_fallback_dev_session_does_not_overwrite_a_config_that_recovered() {
    let _guard = provenance_lock();
    let dir = temp_dir("save-provenance");
    let path = dir.join("config.json");
    write_atomic(&path, &registered()).unwrap();

    remember_provenance(Provenance::Fallback);
    let result = save_standalone_config(&path, &AppConfig::default());
    remember_provenance(Provenance::Authoritative);

    assert!(
        result.is_err(),
        "a fallback session must not overwrite a readable config"
    );
    let LoadOutcome::Loaded(on_disk) = read_from(&path) else {
        panic!("the untouched config should still load");
    };
    assert!(
        on_disk.translation.foundry_local.managed_runtime.is_some(),
        "the config on disk should be exactly as it was"
    );
}

// The loss this fix exists to stop, in the shape the browser actually produces.
// The browser frontend POSTs languages, interval, overlay and translation - and
// nothing else. Container-level `default` fills every absent key with a blank, so before
// the preserve step a single settings save from dev mode blanked the capture
// region in the installed app's config, and `refresh_backup` then copied the
// blanked version over the backup as well.
#[test]
fn a_dev_mode_settings_save_keeps_the_capture_region_and_window_geometry() {
    let _guard = provenance_lock();
    remember_provenance(Provenance::Authoritative);
    let dir = temp_dir("save-preserves-app-owned");
    let path = dir.join("config.json");

    let mut established = registered();
    established.last_capture_region = Some(CaptureRegion {
        x: 220,
        y: 880,
        width: 1481,
        height: 220,
    });
    established.last_capture_scale_factor = Some(2.0);
    established.window_preferences = WindowPreferences {
        width: Some(1100),
        height: Some(760),
        x: Some(64),
        y: Some(48),
        scale_factor: Some(2.0),
        is_maximized: false,
    };
    write_atomic(&path, &established).unwrap();

    // What the settings form sends: no region, no scale, no geometry.
    let mut from_the_browser = AppConfig::default();
    from_the_browser.target_language = "ja-JP".to_string();
    save_standalone_config(&path, &from_the_browser).expect("save should succeed");

    let LoadOutcome::Loaded(on_disk) = read_from(&path) else {
        panic!("the saved config should load");
    };
    assert_eq!(on_disk.target_language, "ja-JP", "the edit should land");
    assert_eq!(
        on_disk.last_capture_region.map(|region| region.width),
        Some(1481),
        "the capture region should survive a settings save that never carried it"
    );
    assert_eq!(on_disk.last_capture_scale_factor, Some(2.0));
    assert_eq!(on_disk.window_preferences.width, Some(1100));

    // And the backup must not be refreshed from the blanked version either.
    let LoadOutcome::Loaded(backup) = read_from(&backup_path(&path)) else {
        panic!("the backup should load");
    };
    assert!(backup.last_capture_region.is_some());
}

// Preserving must not freeze these. Moving the capture region is an ordinary
// thing to do, and the Tauri path saves it by carrying a value - so a supplied
// value has to win over the one on disk.
#[test]
fn a_save_that_carries_a_region_still_replaces_the_one_on_disk() {
    let _guard = provenance_lock();
    remember_provenance(Provenance::Authoritative);
    let dir = temp_dir("save-updates-app-owned");
    let path = dir.join("config.json");

    let mut established = registered();
    established.last_capture_region = Some(CaptureRegion {
        x: 0,
        y: 0,
        width: 100,
        height: 100,
    });
    write_atomic(&path, &established).unwrap();

    let mut moved = registered();
    moved.last_capture_region = Some(CaptureRegion {
        x: 300,
        y: 900,
        width: 1200,
        height: 200,
    });
    save_standalone_config(&path, &moved).expect("save should succeed");

    let LoadOutcome::Loaded(on_disk) = read_from(&path) else {
        panic!("the saved config should load");
    };
    assert_eq!(
        on_disk.last_capture_region.map(|region| region.width),
        Some(1200),
        "a region the caller supplied must replace the stored one"
    );
}

#[test]
fn production_standalone_config_keeps_the_existing_namespace() {
    assert_eq!(
        standalone_config_path_for(
            AppProfile::Production,
            Some(r"C:\Users\tester\AppData\Roaming")
        ),
        PathBuf::from(r"C:\Users\tester\AppData\Roaming")
            .join("com.meowcal.sub")
            .join("config.json")
    );
}

#[test]
fn development_standalone_config_uses_a_distinct_namespace() {
    assert_eq!(
        standalone_config_path_for(
            AppProfile::Development,
            Some(r"C:\Users\tester\AppData\Roaming")
        ),
        PathBuf::from(r"C:\Users\tester\AppData\Roaming")
            .join("com.meowcal.sub.dev")
            .join("config.json")
    );
}

#[test]
fn development_standalone_config_has_a_distinct_fallback() {
    assert_eq!(
        standalone_config_path_for(AppProfile::Development, None),
        PathBuf::from("config.dev.json")
    );
}

#[cfg(all(target_os = "windows", debug_assertions))]
#[test]
fn a_debug_standalone_server_uses_the_development_namespace() {
    let path = standalone_config_path();
    let expected = match std::env::var_os("APPDATA") {
        Some(appdata) => PathBuf::from(appdata)
            .join("com.meowcal.sub.dev")
            .join("config.json"),
        None => PathBuf::from("config.dev.json"),
    };
    assert_eq!(path, expected);
}

// The backup is what the recovery above restores from, so the dev server must
// keep it current for the same reason the app does.
#[test]
fn a_dev_mode_save_refreshes_the_backup_when_it_is_authoritative() {
    let _guard = provenance_lock();
    remember_provenance(Provenance::Authoritative);
    let dir = temp_dir("save-backup");
    let path = dir.join("config.json");
    write_atomic(&path, &AppConfig::default()).unwrap();

    let mut later = registered();
    later.target_language = "ko-KR".to_string();
    save_standalone_config(&path, &later).expect("save should succeed");

    let LoadOutcome::Loaded(backup) = read_from(&backup_path(&path)) else {
        panic!("the backup should load");
    };
    assert_eq!(backup.target_language, "ko-KR");
}
