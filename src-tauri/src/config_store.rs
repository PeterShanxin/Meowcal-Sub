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

/// Save config to disk atomically, never dropping a registration still on disk.
pub fn save_config(app: &tauri::AppHandle, config: &AppConfig) -> Result<(), String> {
    let path = get_config_path(app)?;
    let mut config = config.clone();
    preserve_runtime_from_disk(&path, &mut config);
    write_atomic(&path, &config)?;
    // Keep the fallback current. Refreshing only at startup meant a tray app left
    // running for days backed up its state once, so a corruption on Wednesday
    // restored Monday - silently rolling back every setting and engine change
    // made in between, and then overwriting them. A backup that just came from a
    // successful write is well-formed by construction, so this cannot poison it.
    refresh_backup(&path);
    Ok(())
}

/// Save, and say so when it fails.
///
/// The background save sites - the capture region, the window geometry - had no
/// caller able to act on an error and so discarded it. That is the user-visible
/// half of #64 through a different door: a region set once and silently gone at
/// the next launch. Nothing here can recover, but it must not be silent.
pub fn save_or_warn(app: &tauri::AppHandle, config: &AppConfig, what: &str) {
    if let Err(error) = save_config(app, config) {
        tracing::warn!("Could not save {what}: {error}");
    }
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

fn sibling(config_path: &Path, name: &str) -> PathBuf {
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
            config.normalize();
            LoadOutcome::Loaded(Box::new(config))
        }
        Err(error) => LoadOutcome::Corrupt(format!("is not valid config JSON: {error}")),
    }
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
                refresh_backup(path);
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
        if let Err(error) = write_atomic(path, &config) {
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

/// Keep the last-known-good copy in step with a config that just parsed.
///
/// Staged and renamed rather than copied straight over. A plain `fs::copy` can
/// be interrupted halfway, leaving a truncated backup - and the backup would
/// then fail in exactly the interruption class it exists to survive, so the next
/// corrupt config would fall through to defaults with no fallback at all.
///
/// A failure to refresh is logged rather than propagated: the config itself
/// loaded fine, so the app should still start. But it must not pass silently,
/// because it means the recovery net is not there.
fn refresh_backup(path: &Path) {
    let backup = backup_path(path);
    let staged = sibling(path, "config.bak.tmp.json");
    let result = fs::copy(path, &staged).and_then(|_| fs::rename(&staged, &backup));
    if let Err(error) = result {
        let _ = fs::remove_file(&staged);
        tracing::warn!("Could not refresh the config backup: {error}");
    }
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
    let temp = staging_path(path);
    if let Err(error) = write_and_sync(&temp, json.as_bytes()) {
        let _ = fs::remove_file(&temp);
        return Err(format!("Failed to stage config: {error}"));
    }
    fs::rename(&temp, path).map_err(|error| {
        // The staged file is useless once the rename failed, and leaving it
        // behind makes the next write look like it interrupted itself.
        let _ = fs::remove_file(&temp);
        format!("Failed to replace config: {error}")
    })
}

/// Write a file and get it onto the disk before anyone renames it into place.
///
/// `fs::write` returns once the bytes are in the OS cache. The rename can then
/// reach the NTFS metadata journal while the data extents have not been flushed,
/// so a power loss leaves a full-length `config.json` full of zeros. That is
/// exactly the "exists but reads as nothing" shape this module treats as
/// corruption - so without the flush, the promise of "wholly old or wholly new"
/// is not one this code actually keeps.
fn write_and_sync(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    use std::io::Write;

    let mut file = std::fs::File::create(path)?;
    file.write_all(bytes)?;
    file.sync_all()
}

/// A staging name no concurrent save will also be using.
///
/// Saves are not serialised: autosave from the UI and the window-geometry
/// persistence both reach `save_config`, so two can overlap. Sharing one
/// `config.tmp.json` let the second writer overwrite the first's staged JSON
/// before its rename ran - so a save could report success having written the
/// other one's state - or steal the file outright and make the rename fail.
/// A per-save name removes the interference entirely.
fn staging_path(path: &Path) -> PathBuf {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let ticket = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    sibling(
        path,
        &format!("config.tmp.{}.{ticket}.json", std::process::id()),
    )
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
