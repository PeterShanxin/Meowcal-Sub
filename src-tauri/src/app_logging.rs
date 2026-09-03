//! Application log setup.
//!
//! Owns where session logs go, which filter selects them, and how old log
//! files are retired. `init` is called once at startup; the filter and
//! directory decisions are split from their environment lookups so they can be
//! tested without mutating the process environment out from under parallel
//! tests (the same pattern as `env_flags`).

use crate::app_profile::AppProfile;
use std::path::PathBuf;
use std::time::{Duration, SystemTime};
use tracing_subscriber::{EnvFilter, FmtSubscriber};

/// How long a session log file is kept before cleanup.
const LOG_RETENTION_DAYS: u64 = 7;

/// The filter used when no environment override parses.
const DEFAULT_LOG_FILTER: &str = "meowcal_sub=debug,translation_io=info,tauri=info,axum=info,tower_http=info,hyper=warn,hyper_util=warn,reqwest=warn";

/// Install the tracing subscriber for this process.
///
/// HTTP-only mode writes to the console; normal mode writes one session log
/// file under the resolved log directory, retiring files older than
/// `LOG_RETENTION_DAYS` first. The non-blocking writer guard is leaked
/// deliberately: `main` runs for the whole app lifetime and dropping the
/// guard would stop log flushing mid-session.
pub fn init(http_only_mode: bool) {
    if http_only_mode {
        // Log to console in HTTP-only mode for easier debugging
        let filter = resolve_log_filter();
        let subscriber = FmtSubscriber::builder()
            .with_env_filter(filter)
            .with_ansi(true)
            .pretty()
            .finish();
        tracing::subscriber::set_global_default(subscriber)
            .expect("setting default subscriber failed");
        return;
    }

    // Log to file in normal mode - per-session with unique timestamp
    // Create logs directory if it doesn't exist
    let logs_dir = resolve_log_dir();
    std::fs::create_dir_all(&logs_dir).ok();

    // Clean up old log files (older than LOG_RETENTION_DAYS days)
    cleanup_old_logs(&logs_dir, LOG_RETENTION_DAYS);

    // Generate session-unique log filename with full timestamp
    let now = chrono::Local::now();
    let log_filename = format!("meowcal-sub_{}.log", now.format("%Y-%m-%d_%H-%M-%S"));
    let log_path = logs_dir.join(&log_filename);

    // Create a file appender for this specific session
    let file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
        .expect("Failed to open log file");

    let (non_blocking, guard) = tracing_appender::non_blocking(file);

    let filter = resolve_log_filter();
    let subscriber = FmtSubscriber::builder()
        .with_env_filter(filter)
        .with_writer(non_blocking)
        .with_ansi(false) // File logs shouldn't have color codes
        .pretty()
        .finish();

    tracing::subscriber::set_global_default(subscriber).expect("setting default subscriber failed");

    // INFO: We must keep the guard alive!
    // We'll leak it since main() runs for the whole app duration
    Box::leak(Box::new(guard));
}

/// The log filter in effect: the first environment override that parses wins,
/// otherwise the built-in default.
///
/// Malformed overrides are reported to stderr and skipped, so a broken
/// `RUST_LOG` never silences the app's own diagnostics.
pub fn resolve_log_filter() -> EnvFilter {
    let custom = std::env::var("MEOWCAL_LOG_FILTER").ok();
    let rust_log = std::env::var("RUST_LOG").ok();
    let (directive, rejected) = choose_filter_directive(custom.as_deref(), rust_log.as_deref());
    for (bad, error) in rejected {
        eprintln!("Invalid log filter '{}': {}", bad, error);
    }
    EnvFilter::new(&directive)
}

/// Pick the filter directive: the first non-empty override that parses, else
/// the default. Rejected overrides are returned for the caller to report.
///
/// Separate from `resolve_log_filter` so the precedence and fallback rules can
/// be tested without touching the process environment.
fn choose_filter_directive(
    custom: Option<&str>,
    rust_log: Option<&str>,
) -> (String, Vec<(String, String)>) {
    let mut rejected = Vec::new();
    for candidate in [custom, rust_log].into_iter().flatten() {
        let trimmed = candidate.trim();
        if trimmed.is_empty() {
            continue;
        }

        match EnvFilter::try_new(trimmed) {
            Ok(_) => return (trimmed.to_string(), rejected),
            Err(err) => rejected.push((trimmed.to_string(), err.to_string())),
        }
    }

    (DEFAULT_LOG_FILTER.to_string(), rejected)
}

/// Where session logs go: `MEOWCAL_LOG_DIR` when set, else the current
/// profile's `%APPDATA%\<identifier>\logs` directory.
pub fn resolve_log_dir() -> PathBuf {
    choose_log_dir(
        std::env::var("MEOWCAL_LOG_DIR").ok().as_deref(),
        std::env::var("APPDATA").ok().as_deref(),
        AppProfile::current(),
    )
}

/// The log directory decision itself, testable without touching the
/// environment.
///
/// A non-blank `MEOWCAL_LOG_DIR` wins. Otherwise `APPDATA` is used verbatim
/// when present — including a blank value. The production fallback remains the
/// relative `logs` directory; development has its own relative namespace when
/// no app-data root is available.
fn choose_log_dir(custom: Option<&str>, appdata: Option<&str>, profile: AppProfile) -> PathBuf {
    if let Some(dir) = custom {
        let trimmed = dir.trim();
        if !trimmed.is_empty() {
            return PathBuf::from(trimmed);
        }
    }

    if let Some(appdata) = appdata {
        return PathBuf::from(appdata)
            .join(profile.identifier())
            .join("logs");
    }

    if profile == AppProfile::Development {
        return PathBuf::from(profile.identifier()).join("logs");
    }

    PathBuf::from("logs")
}

/// Delete `.log` files under `logs_dir` whose last modification predates
/// `max_age_days`.
fn cleanup_old_logs(logs_dir: &std::path::Path, max_age_days: u64) {
    let max_age = Duration::from_secs(max_age_days * 24 * 60 * 60);
    let now = SystemTime::now();

    let entries = match std::fs::read_dir(logs_dir) {
        Ok(entries) => entries,
        Err(_) => return, // Directory doesn't exist or can't be read
    };

    for entry in entries.flatten() {
        let path = entry.path();

        // Only process .log files
        if path.extension().is_none_or(|ext| ext != "log") {
            continue;
        }

        // Check file modification time
        let metadata = match std::fs::metadata(&path) {
            Ok(m) => m,
            Err(_) => continue,
        };

        let modified = match metadata.modified() {
            Ok(t) => t,
            Err(_) => continue,
        };

        // Delete if older than max_age
        if let Ok(age) = now.duration_since(modified) {
            if age > max_age {
                let _ = std::fs::remove_file(&path);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::OnceLock;
    use tracing_subscriber::layer::Filter;

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
}
