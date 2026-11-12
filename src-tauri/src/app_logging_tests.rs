use super::*;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::OnceLock;

const CUSTOM: &str = "meowcal_sub=warn";
const OTHER: &str = "tauri=info";
const MALFORMED: &str = "tauri=banana";

#[test]
fn the_first_valid_override_wins() {
    let (directive, rejected) = choose_filter_directive(Some(CUSTOM), Some(OTHER));
    assert_eq!(directive, CUSTOM);
    assert!(rejected.is_empty());
}

#[test]
fn a_malformed_override_falls_through_to_the_next() {
    let (directive, rejected) = choose_filter_directive(Some(MALFORMED), Some(OTHER));
    assert_eq!(directive, OTHER);
    assert_eq!(rejected.len(), 1);
    assert_eq!(rejected[0].0, MALFORMED);
}

#[test]
fn malformed_overrides_are_reported_but_never_used() {
    let (directive, rejected) = choose_filter_directive(Some(MALFORMED), None);
    assert_eq!(directive, DEFAULT_LOG_FILTER);
    assert_eq!(rejected.len(), 1);
}

#[test]
fn a_blank_override_is_skipped_not_rejected() {
    let (directive, rejected) = choose_filter_directive(Some("   "), Some(OTHER));
    assert_eq!(directive, OTHER);
    assert!(rejected.is_empty());
}

#[test]
fn with_no_overrides_the_default_filter_is_used() {
    let (directive, rejected) = choose_filter_directive(None, None);
    assert_eq!(directive, DEFAULT_LOG_FILTER);
    assert!(rejected.is_empty());
}

#[test]
fn the_chosen_directive_actually_selects_its_target() {
    let filter = EnvFilter::new(&choose_filter_directive(Some("meowcal_sub=debug"), None).0);
    assert_eq!(
        filter.max_level_hint(),
        Some(tracing::level_filters::LevelFilter::DEBUG)
    );
}

#[test]
fn an_explicit_override_is_a_real_choice_not_the_default() {
    // The default keeps its most verbose directive at `debug`; an override
    // of `tauri=off` must produce a genuinely stricter filter for the
    // override to mean anything.
    let default = EnvFilter::new(DEFAULT_LOG_FILTER);
    let overridden = EnvFilter::new(&choose_filter_directive(Some("tauri=off"), None).0);
    assert_ne!(default.max_level_hint(), overridden.max_level_hint());
    assert_eq!(
        overridden.max_level_hint(),
        Some(tracing::level_filters::LevelFilter::OFF)
    );
}

#[test]
fn a_custom_log_dir_wins_over_appdata() {
    assert_eq!(
        choose_log_dir(
            Some("C:\\logs"),
            Some("C:\\Users\\tester\\AppData\\Roaming"),
            AppProfile::Production,
        ),
        PathBuf::from("C:\\logs")
    );
}

#[test]
fn a_blank_custom_log_dir_falls_back_to_appdata() {
    assert_eq!(
        choose_log_dir(
            Some("   "),
            Some("C:\\Users\\tester\\AppData\\Roaming"),
            AppProfile::Production,
        ),
        PathBuf::from("C:\\Users\\tester\\AppData\\Roaming")
            .join("com.meowcal.sub")
            .join("logs")
    );
}

#[test]
fn appdata_logs_land_under_the_app_folder() {
    assert_eq!(
        choose_log_dir(
            None,
            Some("C:\\Users\\tester\\AppData\\Roaming"),
            AppProfile::Production,
        ),
        PathBuf::from("C:\\Users\\tester\\AppData\\Roaming")
            .join("com.meowcal.sub")
            .join("logs")
    );
}

#[test]
fn without_appdata_the_relative_logs_dir_is_used() {
    assert_eq!(
        choose_log_dir(None, None, AppProfile::Production),
        PathBuf::from("logs")
    );
}

#[test]
fn a_blank_appdata_is_passed_through_unchanged() {
    assert_eq!(
        choose_log_dir(None, Some("   "), AppProfile::Production),
        PathBuf::from("   ").join("com.meowcal.sub").join("logs")
    );
}

#[test]
fn development_logs_use_the_development_namespace() {
    assert_eq!(
        choose_log_dir(
            None,
            Some("C:\\Users\\tester\\AppData\\Roaming"),
            AppProfile::Development,
        ),
        PathBuf::from("C:\\Users\\tester\\AppData\\Roaming")
            .join("com.meowcal.sub.dev")
            .join("logs")
    );
}

#[test]
fn development_without_appdata_keeps_logs_separate() {
    assert_eq!(
        choose_log_dir(None, None, AppProfile::Development),
        PathBuf::from("com.meowcal.sub.dev").join("logs")
    );
}

fn set_modified(path: &Path, age: Duration) -> std::io::Result<()> {
    // Windows refuses to set times on a read-only handle, so open for write.
    let file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(path)?;
    let times = std::fs::FileTimes::new().set_modified(SystemTime::now() - age);
    file.set_times(times)
}

fn log_temp_dir() -> std::io::Result<PathBuf> {
    static COUNTER: OnceLock<AtomicU64> = OnceLock::new();
    let dir = std::env::temp_dir().join(format!(
        "meowcal-app-logging-{}-{}",
        std::process::id(),
        COUNTER
            .get_or_init(|| AtomicU64::new(0))
            .fetch_add(1, Ordering::Relaxed)
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

#[test]
fn logs_older_than_retention_are_deleted() -> std::io::Result<()> {
    let dir = log_temp_dir()?;
    let old = dir.join("old.log");
    let fresh = dir.join("fresh.log");
    let not_a_log = dir.join("notes.txt");
    std::fs::write(&old, "old")?;
    std::fs::write(&fresh, "fresh")?;
    std::fs::write(&not_a_log, "notes")?;

    set_modified(&old, Duration::from_secs(8 * 24 * 60 * 60))?;
    set_modified(&fresh, Duration::from_secs(60 * 60))?;

    cleanup_old_logs(&dir, 7);

    assert!(!old.exists(), "aged .log file must be deleted");
    assert!(fresh.exists(), "recent .log file must be kept");
    assert!(not_a_log.exists(), "non-.log files must never be deleted");

    let _ = std::fs::remove_dir_all(&dir);
    Ok(())
}

#[test]
fn a_missing_log_dir_is_not_an_error() {
    cleanup_old_logs(Path::new("C:\\definitely\\not\\a\\real\\meowcal\\dir"), 7);
}

#[test]
fn marker_uses_runtime_identity_not_rust_build_profile() {
    assert_eq!(label_for_identifier("com.meowcal.sub.dev"), "dev");
    assert_eq!(label_for_identifier("com.meowcal.sub.store.dev"), "dev");
    assert_eq!(label_for_identifier("com.meowcal.sub"), "prod");
}
