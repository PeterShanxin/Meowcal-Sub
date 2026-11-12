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
use tracing_subscriber::{layer::SubscriberExt, Layer, EnvFilter};

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
pub fn init(http_only_mode: bool, identifier: &str) {
    let _ = PROFILE_LABEL.set(label_for_identifier(identifier));
    if http_only_mode {
        // Log to console in HTTP-only mode for easier debugging
        let filter = resolve_log_filter();
        let subscriber = tracing_subscriber::registry().with(filter).with(
            tracing_subscriber::fmt::layer().with_ansi(true).pretty()
                .with_timer(ProfileTimer)
                .with_filter(tracing_subscriber::filter::dynamic_filter_fn(|meta, _| crate::logging_settings::enabled(meta.level())))
        );
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
    let log_filename = format!("meowcal-sub_{}_{}_{}.log", profile_label(), now.format("%Y-%m-%d_%H-%M-%S"), std::process::id());
    let log_path = logs_dir.join(&log_filename);

    // Create a file appender for this specific session
    let file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
        .expect("Failed to open log file");

    let (non_blocking, guard) = tracing_appender::non_blocking(file);

    let filter = resolve_log_filter();
    let subscriber = tracing_subscriber::registry().with(filter).with(
        tracing_subscriber::fmt::layer().with_writer(non_blocking).with_ansi(false).pretty()
            .with_timer(ProfileTimer)
            .with_filter(tracing_subscriber::filter::dynamic_filter_fn(|meta, _| crate::logging_settings::enabled(meta.level())))
    );

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

static PROFILE_LABEL: std::sync::OnceLock<&'static str> = std::sync::OnceLock::new();
pub(crate) fn profile_label() -> &'static str { PROFILE_LABEL.get().copied().unwrap_or("unknown") }
fn label_for_identifier(identifier: &str) -> &'static str {
    if identifier.ends_with(".dev") { "dev" } else { "prod" }
}

struct ProfileTimer;
impl tracing_subscriber::fmt::time::FormatTime for ProfileTimer {
    fn format_time(&self, writer: &mut tracing_subscriber::fmt::format::Writer<'_>) -> std::fmt::Result {
        write!(writer, "[{}] {}", profile_label(), chrono::Local::now().format("%Y-%m-%dT%H:%M:%S%.3f%:z"))
    }
}
#[cfg(test)]
#[path = "app_logging_tests.rs"]
mod tests;
