// =============================================================================
// ENGINE_RECOVERY.RS - finding an engine that is installed but unregistered
// =============================================================================
// Issue #65: the engine's install location was only ever derivable from
// `managed_runtime`, so a config that lost that record also lost 1.1 GB of
// verified files sitting on disk. Setup then fell back to the default cache
// directory, found nothing there, and started downloading the whole engine
// again - into a different tree from the one the viewer had chosen.
//
// The files are self-describing: a cache root plus the shipped manifest gives
// the exact executable and model paths, their sizes, and their hashes. So a lost
// registration is recoverable without downloading anything, provided we still
// know where to look - which is what `engine_cache_root` on `FoundryLocalConfig`
// is for, and why it is stored separately from the record it has to outlive.
//
// Recovery is deliberately conservative: it adopts an install only when both
// artifacts verify against the manifest by size and SHA-256, the same standard
// the installer applies. Adoption is not a report - it writes the paths into
// `managed_runtime` and startup then launches the executable - so a size match
// is not a strong enough claim to run something on.
// =============================================================================

use crate::config::FoundryLocalConfig;
use crate::engine_manifest::EngineManifest;
use crate::hy_mt_runtime::HyMtInstallPaths;
use std::path::{Path, PathBuf};

/// Where an engine may be installed, best guess first.
///
/// The recorded root comes first because it is where the viewer put it; the
/// default is a fallback for installs predating `engine_cache_root`, which have
/// no recorded root but are very often in the default place anyway.
pub fn candidate_roots(recorded: Option<&str>, default_root: &Path) -> Vec<PathBuf> {
    let mut roots: Vec<PathBuf> = Vec::new();
    if let Some(recorded) = recorded.map(str::trim).filter(|root| !root.is_empty()) {
        roots.push(PathBuf::from(recorded));
    }
    let default_root = default_root.to_path_buf();
    if !roots.contains(&default_root) {
        roots.push(default_root);
    }
    roots
}

/// The first candidate root holding an install that verifies against the manifest.
///
/// Both artifacts are checked by size *and* SHA-256, the same standard the
/// installer applies before it registers anything. Size alone is what
/// `is_complete` uses, and it is enough for reporting status - but adoption
/// writes these paths into `managed_runtime` and startup then launches the
/// executable, so a same-sized corrupt or tampered file under the recorded root
/// would be trusted and run rather than repaired. Hashing costs seconds on the
/// 1.1 GB model; it is paid once, only when a registration has been lost.
pub fn find_installed(roots: &[PathBuf], manifest: &EngineManifest) -> Option<HyMtInstallPaths> {
    use crate::engine_artifact_io::file_matches_blocking;

    let runtime = manifest.runtime_for_current_arch().ok()?;
    roots.iter().find_map(|root| {
        let paths = HyMtInstallPaths::from_cache_root(root, manifest, runtime);
        // Sizes first: they reject a wrong or absent tree instantly, so the
        // expensive hash only runs on a candidate that could plausibly be right.
        if !paths.is_complete(manifest, runtime) {
            return None;
        }
        let verified = file_matches_blocking(
            &paths.executable,
            runtime.executable.size_bytes,
            &runtime.executable.sha256,
        ) && file_matches_blocking(
            &paths.model,
            manifest.model.artifact.size_bytes,
            &manifest.model.artifact.sha256,
        );
        if !verified {
            tracing::warn!(
                "An install at {} is the right size but does not match the manifest; \
                 leaving it for setup to repair rather than adopting it",
                paths.root.display()
            );
            return None;
        }
        Some(paths)
    })
}

/// Rebuild a lost registration in place, reporting whether anything changed.
///
/// Does nothing when a runtime is already registered: an install the app is
/// using must never be swapped out from under it by a directory scan.
pub fn restore_registration(
    config: &mut FoundryLocalConfig,
    manifest: &EngineManifest,
    default_root: &Path,
) -> Option<PathBuf> {
    if config.managed_runtime.is_some() {
        return None;
    }

    let roots = candidate_roots(config.engine_cache_root.as_deref(), default_root);
    let paths = find_installed(&roots, manifest)?;
    let runtime = paths.managed_config(manifest);

    config.endpoint_url = Some(crate::hy_mt_runtime::endpoint_url(&runtime));
    config.model = Some(manifest.model.id.clone());
    config.managed_runtime = Some(runtime);
    // Record the root that worked, so the next recovery does not have to guess
    // and setup never re-downloads into a different tree.
    config.engine_cache_root = cache_root_of(&paths);
    Some(paths.root)
}

/// The cache root an install should use, preferring what the runtime record
/// says and falling back to the root recorded independently of it.
///
/// `None` means "no idea" - the installer then picks the platform default.
pub fn install_cache_root(config: &FoundryLocalConfig) -> Option<String> {
    // Each candidate is filtered on its own. Filtering the resolved `Option`
    // instead meant a runtime record pointing at a detached drive produced
    // `Some`, skipped `or_else` entirely, and was then filtered to `None` - so
    // the independently recorded root, which may well be on an attached drive,
    // was never tried and setup re-downloaded 1.1 GB (#69 review).
    let from_runtime = config
        .managed_cache_root()
        .map(|path| path.to_string_lossy().to_string())
        .filter(|root| volume_exists(Path::new(root)));
    from_runtime.or_else(|| {
        config
            .engine_cache_root
            .as_deref()
            .map(str::trim)
            .filter(|root| !root.is_empty())
            .map(str::to_string)
            .filter(|root| volume_exists(Path::new(root)))
    })
}

/// Whether the volume a path sits on is present.
fn volume_exists(root: &Path) -> bool {
    match root.components().next() {
        // `D:\...` - the prefix is the drive, and it either exists or does not.
        // Built as a string rather than with `join`, which would treat a leading
        // separator as a fresh absolute path and check the current drive instead.
        Some(std::path::Component::Prefix(prefix)) => {
            PathBuf::from(format!("{}\\", prefix.as_os_str().to_string_lossy())).is_dir()
        }
        // Relative or otherwise unusual: leave the judgement to the installer,
        // which rejects non-absolute roots with a clear message of its own.
        _ => true,
    }
}

/// The cache root that contains an install, ready to record.
///
/// `HyMtInstallPaths::root` is the `meowcal-sub` directory *inside* the cache
/// root, so the parent is what `from_cache_root` expects back.
pub fn cache_root_of(paths: &HyMtInstallPaths) -> Option<String> {
    paths
        .root
        .parent()
        .map(|root| root.to_string_lossy().to_string())
}

/// Record where a healthy install lives, for configs written before
/// `engine_cache_root` existed. Reports whether anything was added.
///
/// Without this the fix protects only installs made after it shipped: existing
/// ones carry a runtime record and no recorded root, so the first config
/// problem would still leave recovery searching the default directory.
pub fn backfill_cache_root(config: &mut FoundryLocalConfig) -> bool {
    // Blank counts as missing. A recorded-but-empty root is what an older build
    // leaves behind, and returning early on it left custom-root installs with
    // nothing to recover from - `candidate_roots` ignores a blank value, so
    // recovery would search only the default cache (#69 review).
    let recorded = config
        .engine_cache_root
        .as_deref()
        .map(str::trim)
        .filter(|root| !root.is_empty());
    if recorded.is_some() {
        return false;
    }
    let Some(root) = config.managed_cache_root() else {
        return false;
    };
    config.engine_cache_root = Some(root.to_string_lossy().to_string());
    true
}

/// Load persisted settings, re-adopting an engine that is installed but no
/// longer registered, and persisting the rebuilt registration.
///
/// Recovery is folded into loading because there is no useful moment between
/// the two: a config read without it sends the viewer to first-run setup for an
/// engine already on disk (#65).
///
/// Every failure here is logged and swallowed on purpose. Recovery is a bonus,
/// and a viewer who genuinely has no engine must still reach setup.
pub fn load_with_engine(app: &tauri::AppHandle) -> crate::config::AppConfig {
    use tauri::Manager;

    let mut config = crate::config_store::load_config(app);
    if config.translation.foundry_local.managed_runtime.is_some() {
        if backfill_cache_root(&mut config.translation.foundry_local) {
            // Every install predating `engine_cache_root` has a runtime record
            // and no recorded root, so recovery would have nowhere to look the
            // day that record goes. Recorded now, while it can still be derived.
            if let Err(error) = crate::config_save::save_config(app, &config) {
                // Losing this quietly means the next config problem sends setup
                // to the default cache directory and re-downloads 1.1 GB - the
                // #65 outcome the backfill exists to prevent, with no trace of
                // why it did not work.
                tracing::error!("Could not record where the engine is installed: {error}");
            }
        }
        return config;
    }
    // Reported separately: both are infrastructure failures, not "no engine", and
    // silence here sends the viewer to first-run setup for an engine that is
    // sitting on disk with nothing in the log to explain it.
    let manifest = match EngineManifest::shipped() {
        Ok(manifest) => manifest,
        Err(error) => {
            tracing::error!("Cannot check for an installed engine - manifest unreadable: {error}");
            return config;
        }
    };
    let default_root = match app.path().app_cache_dir() {
        Ok(root) => root,
        Err(error) => {
            tracing::error!("Cannot check for an installed engine - no cache directory: {error}");
            return config;
        }
    };
    let Some(root) = restore_registration(
        &mut config.translation.foundry_local,
        &manifest,
        &default_root,
    ) else {
        return config;
    };

    tracing::warn!(
        "Config had no engine registration but a complete install is present at {}; \
         re-adopted it instead of asking for setup",
        root.display()
    );
    if let Err(error) = crate::config_save::save_config(app, &config) {
        // The in-memory recovery still stands for this session.
        tracing::error!("Could not persist the recovered engine registration: {error}");
    }
    config
}

#[cfg(test)]
#[path = "engine_recovery_tests.rs"]
mod tests;
