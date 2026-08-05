// =============================================================================
// CONFIG_SAVE.RS - writing config.json without overwriting something better
// =============================================================================
// Reading lives in `config_store`. Writing lives here because it has a question
// reading does not: whether this session's config is even entitled to replace
// what is on disk.
//
// The #69 review found two ways it was not. Both had the same cause - the
// `Recovery` verdict was logged and then dropped, so every save behaved
// identically whether the config had been read cleanly or reconstructed from a
// fallback:
//
// - Refreshing the backup after a save could copy factory defaults over the last
//   known-good copy. If `config.json` was corrupt *and* `config.bak.json` was
//   briefly locked, the app started on defaults while the backup still held the
//   only real settings - and the next window close destroyed it. That is #64
//   reproduced by the mechanism added to prevent it.
// - A config that was merely locked at startup is served from the backup, and
//   the moment the lock released, any save wrote that stale copy over the real
//   file - with no quarantine, because the locked path deliberately skips it.
//
// `Provenance` is the answer to both: a session that did not read its config
// cleanly may still save, but it may not refresh the backup, and it may never
// overwrite a file that has since become readable.
// =============================================================================

use crate::config::AppConfig;
use crate::config_store::{backup_path, read_from, sibling, LoadOutcome};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

/// Where this session's config came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Provenance {
    /// Read cleanly off disk. It may replace what is there, and the backup may
    /// be refreshed from it.
    Authoritative,
    /// Reconstructed from a backup or from defaults. Usable for the session, but
    /// not evidence of what the user's settings actually are.
    Fallback,
}

/// Set once per launch by `config_store::load_config`.
///
/// A static rather than a field on `AppState` because every save site already
/// reaches `save_config` with nothing but an `AppHandle`, and threading a flag
/// through all of them would put the guard's correctness in the hands of each
/// caller. There is exactly one config per process, so there is exactly one
/// answer.
static SESSION_PROVENANCE_IS_FALLBACK: AtomicBool = AtomicBool::new(false);

pub fn remember_provenance(provenance: Provenance) {
    SESSION_PROVENANCE_IS_FALLBACK.store(provenance == Provenance::Fallback, Ordering::SeqCst);
}

pub fn session_provenance() -> Provenance {
    if SESSION_PROVENANCE_IS_FALLBACK.load(Ordering::SeqCst) {
        Provenance::Fallback
    } else {
        Provenance::Authoritative
    }
}

/// Save config to disk atomically, refusing to make anything worse.
pub fn save_config(app: &tauri::AppHandle, config: &AppConfig) -> Result<(), String> {
    let path = crate::config_store::get_config_path(app)?;
    save_to(&path, config, session_provenance())
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

/// The whole save decision, on a path so it can be tested without a Tauri app.
pub fn save_to(path: &Path, config: &AppConfig, provenance: Provenance) -> Result<(), String> {
    let on_disk = read_from(path);

    // A file we could not read is not a file we may replace. Its bytes may be
    // intact behind a lock, and `write_atomic`'s rename would destroy them.
    if let LoadOutcome::Unreadable(reason) = &on_disk {
        return Err(format!(
            "config.json {reason}, so it was left alone rather than overwritten"
        ));
    }

    // A fallback session holds the backup's contents, not the user's. If the real
    // config is readable again, it is newer than what we are holding and must
    // win - otherwise the lock that lasted 200ms costs every setting changed
    // since the backup was last refreshed.
    if provenance == Provenance::Fallback && matches!(on_disk, LoadOutcome::Loaded(_)) {
        return Err(
            "config.json became readable again, so this session's fallback copy was not \
             written over it"
                .to_string(),
        );
    }

    let mut config = config.clone();
    if let LoadOutcome::Loaded(ref disk) = on_disk {
        preserve_runtime(disk, &mut config);
    }
    write_atomic(path, &config)?;

    // Keep the fallback current, but only from a session that read its config
    // cleanly. Refreshing only at startup meant a tray app left running for days
    // backed up once, so a corruption on Wednesday restored Monday. Refreshing
    // from a fallback session is worse still: it would copy defaults over the
    // one surviving record of the user's settings.
    if provenance == Provenance::Authoritative {
        refresh_backup(path);
    }
    Ok(())
}

/// Refuse to write away an engine registration that is still on disk.
///
/// `FoundryLocalConfig::preserve_managed_runtime_from` already guards settings
/// saves, but it preserves from the in-memory config - which, after a bad load,
/// is the default with no runtime at all. So the guard was blind in exactly the
/// case it existed for, and the first save after a bad load made the loss
/// permanent. Consulting the file closes that: the engine is app-owned, so a
/// runtime present on disk and absent in memory is always the bug, never intent.
fn preserve_runtime(on_disk: &AppConfig, config: &mut AppConfig) {
    if config.translation.foundry_local.managed_runtime.is_some() {
        return;
    }
    config
        .translation
        .foundry_local
        .preserve_managed_runtime_from(&on_disk.translation.foundry_local);
}

/// Keep the last-known-good copy in step with a config that just parsed.
///
/// Staged and renamed rather than copied straight over. A plain `fs::copy` can
/// be interrupted halfway, leaving a truncated backup - and the backup would
/// then fail in exactly the interruption class it exists to survive, so the next
/// corrupt config would fall through to defaults with no fallback at all.
///
/// A failure is logged rather than propagated: the config itself was written, so
/// the save succeeded. But it must not pass silently, because it means the
/// recovery net is not there.
pub fn refresh_backup(path: &Path) {
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
fn staging_path(path: &Path) -> PathBuf {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let ticket = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    sibling(
        path,
        &format!("config.tmp.{}.{ticket}.json", std::process::id()),
    )
}

#[cfg(test)]
#[path = "config_save_tests.rs"]
mod tests;
