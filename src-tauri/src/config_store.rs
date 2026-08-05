// =============================================================================
// CONFIG_STORE.RS - reading and writing config.json without losing it
// =============================================================================
// Issue #64: a config the app could not parse became `AppConfig::default()`
// silently, and the next save wrote those defaults back. One unreadable read
// permanently erased a working engine registration and the viewer's capture
// region, on a machine where every engine file was intact. The app then offered
// first-run setup for an engine that was sitting on disk, fully installed.
//
// Three things went wrong and all three are fixed here:
//
// - Loading could not tell "there is no config yet", where defaults are exactly
//   right, from "there is a config and it is unreadable", where defaults are
//   destruction. Both fell through to the same expression.
// - Saving used `fs::write`, which truncates first. Interrupt it and the file on
//   disk is valid JSON's corpse - which is precisely the input the loader turned
//   into defaults.
// - The guard meant to protect app-owned engine paths reads the *in-memory*
//   config, so once that had been defaulted the guard saw nothing to preserve.
//
// This module works on paths rather than an `AppHandle` so the failures above
// are testable without a running Tauri app - none of them could be reached from
// a test before.
// =============================================================================

use crate::config::AppConfig;
use std::fs;
use std::path::{Path, PathBuf};
use tauri::Manager;

/// Get the config.json path in the app data directory.
pub fn get_config_path(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    let app_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("Failed to resolve app data dir: {}", e))?;
    fs::create_dir_all(&app_dir).map_err(|e| format!("Failed to create app data dir: {}", e))?;
    Ok(app_dir.join("config.json"))
}

/// Load config from disk, reporting anything other than a clean read.
///
/// A startup that silently reset the engine registration and the capture region
/// is the failure being fixed, so every recovery is logged rather than absorbed.
pub fn load_config(app: &tauri::AppHandle) -> AppConfig {
    let path = match get_config_path(app) {
        Ok(path) => path,
        Err(error) => {
            tracing::error!("Config directory unavailable, starting with defaults: {error}");
            return AppConfig::default();
        }
    };

    let (config, recovery) = load_durable(&path);
    // Every save this session is gated on this. A config reconstructed from a
    // backup is usable but is not evidence of the user's settings, so it must
    // never refresh the backup or overwrite a config that becomes readable
    // again - see `config_save::Provenance`.
    crate::config_save::remember_provenance(match recovery {
        Recovery::None | Recovery::FirstRun => crate::config_save::Provenance::Authoritative,
        _ => crate::config_save::Provenance::Fallback,
    });
    // Each message states only what actually happened. Claiming the corrupt file
    // "is kept as config.corrupt.json" when the rename failed sends whoever
    // triages the problem after a file that is not there - and does it in the
    // one case where the original data still exists and could be rescued.
    match recovery {
        Recovery::None => {}
        Recovery::FirstRun => tracing::info!("No config found; starting with defaults"),
        Recovery::RestoredFromBackup(reason) => tracing::warn!(
            "config.json {reason}; restored the last known-good copy and set the \
             unusable file aside as config.corrupt.json"
        ),
        Recovery::UsingBackupUntilReadable(reason) => tracing::warn!(
            "config.json {reason}; using the last known-good copy for this session \
             and leaving the file untouched in case it becomes readable again"
        ),
        Recovery::Defaulted {
            reason,
            quarantined,
        } => {
            let kept = if quarantined {
                "The unusable file is kept as config.corrupt.json"
            } else {
                "The unusable file could not be set aside and is still config.json"
            };
            tracing::error!(
                "config.json {reason} and no backup was usable; starting with defaults. {kept}"
            );
        }
    }
    config
}

/// What was found where the config should be.
///
/// The distinction between `Missing` and `Unreadable` is the whole point: they
/// used to be the same branch, and conflating them is what erased a working
/// install.
#[derive(Debug)]
pub enum LoadOutcome {
    /// Parsed and normalized.
    Loaded(Box<AppConfig>),
    /// No file. A first run - defaults are correct here.
    Missing,
    /// A file exists and its contents are wrong. Defaults would be data loss.
    Corrupt(String),
    /// A file exists and could not be read at all - locked, denied, bad sector.
    ///
    /// Kept apart from `Corrupt` because the treatment differs. Corrupt contents
    /// justify quarantining the file and writing a recovery over it; an
    /// unreadable *file* does not, since the bytes may be perfectly good and
    /// merely held open by a backup agent or a scanner. Overwriting there would
    /// destroy a valid config to work around a lock that lasted 200ms.
    Unreadable(String),
}

impl LoadOutcome {
    /// Why the config could not be used, whatever the reason.
    fn failure_reason(&self) -> Option<&str> {
        match self {
            Self::Corrupt(reason) | Self::Unreadable(reason) => Some(reason),
            _ => None,
        }
    }
}

/// Where the last good copy is kept, beside the config itself.
pub fn backup_path(config_path: &Path) -> PathBuf {
    sibling(config_path, "config.bak.json")
}

/// Where an unusable config is moved so it can be inspected rather than lost.
pub fn quarantine_path(config_path: &Path) -> PathBuf {
    sibling(config_path, "config.corrupt.json")
}

pub(crate) fn sibling(config_path: &Path, name: &str) -> PathBuf {
    config_path
        .parent()
        .map(|parent| parent.join(name))
        .unwrap_or_else(|| PathBuf::from(name))
}

/// How many times a read is retried before a lock is called a failure.
///
/// Windows hands out sharing violations freely - a backup agent, OneDrive, or a
/// scanner touching the file at the wrong instant is enough. Treating the first
/// one as "unreadable" would demote a perfectly good config to a stale backup
/// over a lock that lasts a moment.
const READ_ATTEMPTS: usize = 3;
const READ_RETRY_DELAY: std::time::Duration = std::time::Duration::from_millis(120);

/// Read one config file, keeping the outcomes apart.
pub fn read_from(path: &Path) -> LoadOutcome {
    let content = match read_with_retry(path) {
        Ok(content) => content,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return LoadOutcome::Missing,
        // The file is there and we cannot see it. Its bytes may be fine, so this
        // is reported as unreadable rather than corrupt and must not be
        // overwritten - see `LoadOutcome::Unreadable`.
        Err(error) => return LoadOutcome::Unreadable(format!("could not be read: {error}")),
    };

    // An empty file is the signature of an interrupted `fs::write`: truncated,
    // never refilled. Corrupt contents, not an unreadable file.
    if content.trim().is_empty() {
        return LoadOutcome::Corrupt("is empty".to_string());
    }

    match serde_json::from_str::<AppConfig>(&content) {
        Ok(mut config) => {
            // Container-level `serde(default)` lets a config survive a key it
            // does not know, which is the point - but it also means `{}` and any
            // unrelated JSON object deserialize happily into factory defaults,
            // reported as a clean load. That would refresh the backup from
            // nothing and log not a word. A config we wrote always carries at
            // least one key we model.
            if !mentions_a_modelled_setting(&content) {
                return LoadOutcome::Corrupt(
                    "contains no recognisable settings, so it is not a config this app wrote"
                        .to_string(),
                );
            }
            config.normalize();
            LoadOutcome::Loaded(Box::new(config))
        }
        Err(error) => LoadOutcome::Corrupt(format!("is not valid config JSON: {error}")),
    }
}

/// Whether a parsed config actually claims to be one.
fn mentions_a_modelled_setting(content: &str) -> bool {
    const MODELLED: [&str; 8] = [
        "sourceLanguage",
        "targetLanguage",
        "captureIntervalMs",
        "overlay",
        "translation",
        "lastCaptureRegion",
        "windowPreferences",
        "minimizeToTray",
    ];
    serde_json::from_str::<serde_json::Value>(content)
        .ok()
        .as_ref()
        .and_then(serde_json::Value::as_object)
        .is_some_and(|object| MODELLED.iter().any(|key| object.contains_key(*key)))
}

fn read_with_retry(path: &Path) -> std::io::Result<String> {
    let mut last = None;
    for attempt in 0..READ_ATTEMPTS {
        match fs::read_to_string(path) {
            Ok(content) => return Ok(content),
            // A missing file will not appear by waiting, and this is the common
            // first-run path - returning immediately keeps startup quick.
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Err(error),
            Err(error) => {
                if attempt + 1 < READ_ATTEMPTS {
                    std::thread::sleep(READ_RETRY_DELAY);
                }
                last = Some(error);
            }
        }
    }
    Err(last.expect("at least one attempt ran"))
}

/// What a load did, so the caller can say so rather than starting up mute.
#[derive(Debug, PartialEq, Eq)]
pub enum Recovery {
    /// The config was read normally.
    None,
    /// No config existed. Defaults, legitimately.
    FirstRun,
    /// The config was corrupt and the backup was used instead. The corrupt file
    /// is kept at `quarantine_path`, and the recovery has been written back.
    RestoredFromBackup(String),
    /// The config could not be read at all, so the backup is being used for this
    /// session only. Nothing on disk was moved or overwritten - the file may be
    /// perfectly good and merely locked.
    UsingBackupUntilReadable(String),
    /// Nothing usable was found anywhere. Defaults, regrettably.
    Defaulted { reason: String, quarantined: bool },
}

/// Load a config, preferring the backup over defaults when the main file is
/// unusable.
///
/// Returns the recovery that happened alongside the config, because a startup
/// that quietly resets everything is the failure this module exists for - the
/// caller is expected to log it, loudly.
pub fn load_durable(path: &Path) -> (AppConfig, Recovery) {
    let outcome = read_from(path);
    let Some(reason) = outcome.failure_reason().map(str::to_string) else {
        return match outcome {
            LoadOutcome::Loaded(config) => {
                crate::config_save::refresh_backup(path);
                (*config, Recovery::None)
            }
            _ => (AppConfig::default(), Recovery::FirstRun),
        };
    };

    // A file we could not read is not a file we may destroy. Its bytes may be
    // intact behind a scanner's lock, so the backup stands in for this session
    // and the disk is left exactly as found - a later launch can still recover
    // the real thing. Corrupt contents get the opposite treatment: quarantined,
    // and the recovery written back.
    let readable = !matches!(outcome, LoadOutcome::Unreadable(_));

    if let LoadOutcome::Loaded(config) = read_from(&backup_path(path)) {
        if !readable {
            return (*config, Recovery::UsingBackupUntilReadable(reason));
        }
        // Keep the corrupt original: it is the only evidence of what was lost,
        // and the next successful save would otherwise write over it.
        let quarantined = fs::rename(path, quarantine_path(path)).is_ok();
        // Put the recovery back on disk now rather than trusting some later save
        // to do it. Nothing guarantees one runs - a crash, Task Manager, or the
        // update handoff all end the process without saving - and the next
        // launch would then find no config at all, take the `Missing` path, and
        // start from defaults. That would turn a survivable corruption into the
        // very loss this module exists to prevent, one session later.
        if let Err(error) = crate::config_save::write_atomic(path, &config) {
            tracing::error!("Recovered config could not be written back: {error}");
        }
        if !quarantined {
            tracing::warn!("The corrupt config could not be set aside for inspection");
        }
        return (*config, Recovery::RestoredFromBackup(reason));
    }

    // Nothing to fall back on. Still refuse to touch a file we merely could not
    // read - overwriting it would destroy the only copy of the real settings.
    let quarantined = readable && fs::rename(path, quarantine_path(path)).is_ok();
    (
        AppConfig::default(),
        Recovery::Defaulted {
            reason,
            quarantined,
        },
    )
}

#[cfg(test)]
#[path = "config_store_tests.rs"]
mod tests;
