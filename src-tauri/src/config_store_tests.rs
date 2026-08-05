use super::*;
use crate::engine_config::ManagedLocalRuntimeConfig;

fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("meowcal-config-store-{name}"));
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

// The distinction the whole module turns on.
#[test]
fn a_missing_config_is_a_first_run_not_a_failure() {
    let dir = temp_dir("missing");
    let (_, recovery) = load_durable(&dir.join("config.json"));
    assert_eq!(recovery, Recovery::FirstRun);
}

// Issue #64: this used to return defaults, indistinguishable from a first
// run, and the next save erased the engine registration for good.
#[test]
fn an_unparseable_config_is_not_treated_as_absent() {
    let dir = temp_dir("unparseable");
    let path = dir.join("config.json");
    fs::write(&path, "{ this is not json").unwrap();

    assert!(matches!(read_from(&path), LoadOutcome::Unreadable(_)));
}

// The exact shape an interrupted `fs::write` leaves behind.
#[test]
fn a_truncated_config_is_unreadable_rather_than_empty_settings() {
    let dir = temp_dir("truncated");
    let path = dir.join("config.json");
    fs::write(&path, "").unwrap();

    assert!(matches!(read_from(&path), LoadOutcome::Unreadable(_)));
}

#[test]
fn a_good_load_refreshes_the_backup() {
    let dir = temp_dir("backup-refresh");
    let path = dir.join("config.json");
    write_atomic(&path, &registered()).unwrap();

    let (_, recovery) = load_durable(&path);

    assert_eq!(recovery, Recovery::None);
    assert!(backup_path(&path).is_file());
}

// The recovery that saves the install: a corrupt config falls back to the
// last good one rather than to factory defaults.
#[test]
fn a_corrupt_config_falls_back_to_the_backup() {
    let dir = temp_dir("restore");
    let path = dir.join("config.json");
    write_atomic(&path, &registered()).unwrap();
    load_durable(&path); // establishes the backup
    fs::write(&path, "{ truncated").unwrap();

    let (config, recovery) = load_durable(&path);

    assert!(matches!(recovery, Recovery::RestoredFromBackup(_)));
    assert!(config.translation.foundry_local.managed_runtime.is_some());
}

// A recovery held only in memory is lost the moment the process ends without
// saving - a crash, Task Manager, the update handoff. The next launch would then
// find no config, take the `Missing` path, and start from defaults: the very
// loss this module prevents, one session later.
#[test]
fn a_recovered_config_is_written_back_immediately() {
    let dir = temp_dir("write-back");
    let path = dir.join("config.json");
    write_atomic(&path, &registered()).unwrap();
    load_durable(&path); // establishes the backup
    fs::write(&path, "{ truncated").unwrap();

    load_durable(&path);

    // Read the file rather than the return value: the point is that the disk
    // now holds the recovery, with no further save required.
    let LoadOutcome::Loaded(on_disk) = read_from(&path) else {
        panic!("the recovered config should be back on disk");
    };
    assert!(on_disk.translation.foundry_local.managed_runtime.is_some());
}

// The backup is the recovery net; refreshing it with a plain copy could leave it
// truncated by the same interruption it exists to survive.
#[test]
fn refreshing_the_backup_leaves_no_staging_file() {
    let dir = temp_dir("backup-staging");
    let path = dir.join("config.json");
    write_atomic(&path, &registered()).unwrap();

    load_durable(&path);

    assert!(backup_path(&path).is_file());
    assert!(!sibling(&path, "config.bak.tmp.json").exists());
}

// Autosave and window-geometry persistence both reach `save_config`, so two
// saves can overlap. A shared staging name let one overwrite the other's staged
// JSON before its rename ran.
#[test]
fn concurrent_saves_do_not_share_a_staging_file() {
    let dir = temp_dir("staging-unique");
    let path = dir.join("config.json");

    let first = staging_path(&path);
    let second = staging_path(&path);

    assert_ne!(first, second);
}

// Losing the config is bad; losing the evidence of what was lost is worse.
#[test]
fn an_unusable_config_is_quarantined_rather_than_overwritten() {
    let dir = temp_dir("quarantine");
    let path = dir.join("config.json");
    fs::write(&path, "{ truncated").unwrap();

    let (_, recovery) = load_durable(&path);

    assert!(matches!(recovery, Recovery::Defaulted(_)));
    assert!(quarantine_path(&path).is_file());
    assert!(!path.exists());
}

// `autoStart` and `startWithWindows` are written to disk but not modelled by
// `AppConfig`, so every save dropped them. Startup now saves during recovery,
// which would have erased them on the first launch after upgrading.
#[test]
fn settings_the_struct_does_not_model_survive_a_round_trip() {
    let dir = temp_dir("unmodelled");
    let path = dir.join("config.json");
    let mut original = serde_json::to_value(AppConfig::default()).unwrap();
    original["autoStart"] = serde_json::Value::Bool(false);
    original["startWithWindows"] = serde_json::Value::Bool(true);
    fs::write(&path, serde_json::to_string_pretty(&original).unwrap()).unwrap();

    let (loaded, _) = load_durable(&path);
    write_atomic(&path, &loaded).unwrap();

    let round_tripped: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(round_tripped["autoStart"], serde_json::Value::Bool(false));
    assert_eq!(
        round_tripped["startWithWindows"],
        serde_json::Value::Bool(true)
    );
}

#[test]
fn a_written_config_reads_back_identically() {
    let dir = temp_dir("roundtrip");
    let path = dir.join("config.json");
    write_atomic(&path, &registered()).unwrap();

    let LoadOutcome::Loaded(restored) = read_from(&path) else {
        panic!("expected the config to load");
    };
    assert_eq!(
        restored
            .translation
            .foundry_local
            .managed_runtime
            .unwrap()
            .port,
        11_436
    );
}

// No `config.tmp.json` should survive a successful write; a leftover would
// look like an interrupted save to anyone diagnosing the next problem.
#[test]
fn writing_leaves_no_staging_file_behind() {
    let dir = temp_dir("staging");
    let path = dir.join("config.json");
    write_atomic(&path, &registered()).unwrap();

    assert!(!sibling(&path, "config.tmp.json").exists());
}

// Issue #64's permanent-wipe step: a save whose in-memory config was
// defaulted must not clear a registration that is still on disk.
#[test]
fn a_save_cannot_clear_a_runtime_that_is_still_on_disk() {
    let dir = temp_dir("preserve");
    let path = dir.join("config.json");
    write_atomic(&path, &registered()).unwrap();

    let mut defaulted = AppConfig::default();
    preserve_runtime_from_disk(&path, &mut defaulted);

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

// The guard must not invent a registration where none was ever recorded.
#[test]
fn preserving_adds_nothing_when_disk_has_no_runtime() {
    let dir = temp_dir("preserve-empty");
    let path = dir.join("config.json");
    write_atomic(&path, &AppConfig::default()).unwrap();

    let mut incoming = AppConfig::default();
    preserve_runtime_from_disk(&path, &mut incoming);

    assert!(incoming.translation.foundry_local.managed_runtime.is_none());
}
