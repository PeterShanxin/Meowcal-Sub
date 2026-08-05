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
    match recovery {
        Recovery::None => {}
        Recovery::FirstRun => tracing::info!("No config found; starting with defaults"),
        Recovery::RestoredFromBackup(reason) => tracing::warn!(
            "config.json {reason}; restored the last known-good copy. \
             The unusable file is kept as config.corrupt.json"
        ),
        Recovery::Defaulted(reason) => tracing::error!(
            "config.json {reason} and no backup was usable; starting with defaults. \
             The unusable file is kept as config.corrupt.json"
        ),
    }
    config
}

/// Save config to disk atomically, never dropping a registration still on disk.
pub fn save_config(app: &tauri::AppHandle, config: &AppConfig) -> Result<(), String> {
    let path = get_config_path(app)?;
    let mut config = config.clone();
    preserve_runtime_from_disk(&path, &mut config);
    write_atomic(&path, &config)
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
    /// A file exists and could not be used. Defaults would be data loss.
    Unreadable(String),
}

/// Where the last good copy is kept, beside the config itself.
pub fn backup_path(config_path: &Path) -> PathBuf {
    sibling(config_path, "config.bak.json")
}

/// Where an unusable config is moved so it can be inspected rather than lost.
pub fn quarantine_path(config_path: &Path) -> PathBuf {
    sibling(config_path, "config.corrupt.json")
}

fn sibling(config_path: &Path, name: &str) -> PathBuf {
    config_path
        .parent()
        .map(|parent| parent.join(name))
        .unwrap_or_else(|| PathBuf::from(name))
}

/// Read one config file, keeping the three outcomes apart.
pub fn read_from(path: &Path) -> LoadOutcome {
    let content = match fs::read_to_string(path) {
        Ok(content) => content,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return LoadOutcome::Missing,
        // Anything else - a lock, a permission problem, a bad sector - means a
        // config probably exists and we simply cannot see it. Defaulting here
        // would discard it on the next save.
        Err(error) => return LoadOutcome::Unreadable(format!("could not be read: {error}")),
    };

    // An empty file is the signature of an interrupted `fs::write`: truncated,
    // never refilled. Reported as unreadable rather than parsed, so recovery
    // runs instead of a silent reset.
    if content.trim().is_empty() {
        return LoadOutcome::Unreadable("is empty".to_string());
    }

    match serde_json::from_str::<AppConfig>(&content) {
        Ok(mut config) => {
            config.normalize();
            LoadOutcome::Loaded(Box::new(config))
        }
        Err(error) => LoadOutcome::Unreadable(format!("is not valid config JSON: {error}")),
    }
}

/// What a load did, so the caller can say so rather than starting up mute.
#[derive(Debug, PartialEq, Eq)]
pub enum Recovery {
    /// The config was read normally.
    None,
    /// No config existed. Defaults, legitimately.
    FirstRun,
    /// The config was unusable and the backup was used instead.
    RestoredFromBackup(String),
    /// The config was unusable and there was no backup. Defaults, regrettably.
    /// The unusable file is kept at `quarantine_path` rather than overwritten.
    Defaulted(String),
}

/// Load a config, preferring the backup over defaults when the main file is
/// unusable.
///
/// Returns the recovery that happened alongside the config, because a startup
/// that quietly resets everything is the failure this module exists for - the
/// caller is expected to log it, loudly.
pub fn load_durable(path: &Path) -> (AppConfig, Recovery) {
    let reason = match read_from(path) {
        LoadOutcome::Loaded(config) => {
            // Only refresh the backup from a config that just parsed, so a bad
            // file can never become the fallback for its own recovery.
            let _ = fs::copy(path, backup_path(path));
            return (*config, Recovery::None);
        }
        LoadOutcome::Missing => return (AppConfig::default(), Recovery::FirstRun),
        LoadOutcome::Unreadable(reason) => reason,
    };

    if let LoadOutcome::Loaded(config) = read_from(&backup_path(path)) {
        // Keep the unusable original: it is the only evidence of what was lost,
        // and the next successful save would otherwise write over it.
        let _ = fs::rename(path, quarantine_path(path));
        return (*config, Recovery::RestoredFromBackup(reason));
    }

    let _ = fs::rename(path, quarantine_path(path));
    (AppConfig::default(), Recovery::Defaulted(reason))
}

/// Write the config so that it is either wholly the old one or wholly the new.
///
/// `fs::write` truncates before it writes, so an interruption leaves a file that
/// exists, parses as nothing, and reads as "no settings". Writing a temporary
/// beside the target and renaming over it is atomic on NTFS, so a reader always
/// sees one complete version or the other.
pub fn write_atomic(path: &Path, config: &AppConfig) -> Result<(), String> {
    let mut config = config.clone();
    config.normalize();
    let json = serde_json::to_string_pretty(&config).map_err(|error| error.to_string())?;

    // Same directory as the target: `fs::rename` is only atomic within a volume,
    // and a temp dir can easily be on another one.
    let temp = sibling(path, "config.tmp.json");
    fs::write(&temp, json).map_err(|error| format!("Failed to stage config: {error}"))?;
    fs::rename(&temp, path).map_err(|error| {
        // The staged file is useless once the rename failed, and leaving it
        // behind makes the next write look like it interrupted itself.
        let _ = fs::remove_file(&temp);
        format!("Failed to replace config: {error}")
    })
}

/// Refuse to write away an engine registration that is still on disk.
///
/// `FoundryLocalConfig::preserve_managed_runtime_from` already guards settings
/// saves, but it preserves from the in-memory config - which, after a bad load,
/// is the default with no runtime at all. So the guard was blind in exactly the
/// case it existed for, and the first save after a bad load made the loss
/// permanent.
///
/// Consulting the file closes that. The engine is app-owned: the UI has no way
/// to legitimately clear these fields, so a runtime present on disk and absent
/// in memory is always the bug, never the intent.
pub fn preserve_runtime_from_disk(path: &Path, config: &mut AppConfig) {
    if config.translation.foundry_local.managed_runtime.is_some() {
        return;
    }
    if let LoadOutcome::Loaded(on_disk) = read_from(path) {
        config
            .translation
            .foundry_local
            .preserve_managed_runtime_from(&on_disk.translation.foundry_local);
    }
}

#[cfg(test)]
#[path = "config_store_tests.rs"]
mod tests;
