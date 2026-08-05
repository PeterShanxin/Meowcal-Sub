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
// the exact executable and model paths, and their sizes say whether the install
// is intact. So a lost registration is recoverable without downloading anything,
// provided we still know where to look - which is what `engine_cache_root` on
// `FoundryLocalConfig` is for, and why it is stored separately from the record
// it has to outlive.
//
// Recovery is deliberately conservative: it adopts an install only when both
// artifacts are present at their manifest sizes. Adopting a half-written tree
// would trade a re-download for an engine that fails at translation time, which
// is the worse failure - it looks like a bug rather than a setup step.
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

/// The first candidate root holding a complete install.
///
/// `is_complete` checks both artifacts against their manifest sizes, the same
/// test `managed_hy_mt_status` uses to decide the engine is usable - so anything
/// adopted here will report ready rather than half-installed.
pub fn find_installed(roots: &[PathBuf], manifest: &EngineManifest) -> Option<HyMtInstallPaths> {
    let runtime = manifest.runtime_for_current_arch().ok()?;
    roots.iter().find_map(|root| {
        let paths = HyMtInstallPaths::from_cache_root(root, manifest, runtime);
        paths.is_complete(manifest, runtime).then_some(paths)
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
    config
        .managed_cache_root()
        .map(|path| path.to_string_lossy().to_string())
        .or_else(|| {
            config
                .engine_cache_root
                .as_deref()
                .map(str::trim)
                .filter(|root| !root.is_empty())
                .map(str::to_string)
        })
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
    if config.engine_cache_root.is_some() {
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
            let _ = crate::config_store::save_config(app, &config);
        }
        return config;
    }
    let (Ok(manifest), Ok(default_root)) = (EngineManifest::shipped(), app.path().app_cache_dir())
    else {
        return config;
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
    if let Err(error) = crate::config_store::save_config(app, &config) {
        // The in-memory recovery still stands for this session.
        tracing::error!("Could not persist the recovered engine registration: {error}");
    }
    config
}

#[cfg(test)]
mod tests {
    use super::*;

    fn default_root() -> PathBuf {
        PathBuf::from(r"C:\Users\someone\AppData\Local\meowcal\cache")
    }

    #[test]
    fn a_recorded_root_is_tried_before_the_default() {
        let roots = candidate_roots(Some(r"D:\foundry-cache"), &default_root());

        assert_eq!(roots[0], PathBuf::from(r"D:\foundry-cache"));
        assert_eq!(roots[1], default_root());
    }

    #[test]
    fn the_default_root_is_the_only_candidate_without_a_record() {
        assert_eq!(candidate_roots(None, &default_root()), vec![default_root()]);
    }

    // Installs predating `engine_cache_root` write an empty string rather than
    // omitting the field; that must not become a candidate path of "".
    #[test]
    fn a_blank_recorded_root_is_ignored() {
        assert_eq!(
            candidate_roots(Some("   "), &default_root()),
            vec![default_root()]
        );
    }

    #[test]
    fn the_default_root_is_not_listed_twice() {
        let recorded = default_root().to_string_lossy().to_string();
        assert_eq!(
            candidate_roots(Some(&recorded), &default_root()),
            vec![default_root()]
        );
    }

    // An engine the app is already using must not be replaced by whatever a
    // directory scan happens to find.
    #[test]
    fn a_registered_runtime_is_left_alone() {
        let manifest = EngineManifest::shipped().expect("shipped manifest");
        let mut config = FoundryLocalConfig {
            managed_runtime: Some(crate::engine_config::ManagedLocalRuntimeConfig {
                kind: "hy-mt".to_string(),
                executable_path: r"D:\engine\llama-server.exe".to_string(),
                model_path: r"D:\engine\HY-MT.gguf".to_string(),
                port: 11_436,
            }),
            ..FoundryLocalConfig::default()
        };

        assert!(restore_registration(&mut config, &manifest, &default_root()).is_none());
    }

    // Configs written before `engine_cache_root` existed carry a runtime record
    // and no recorded root. Without backfilling, the very installs this was
    // built to rescue would still have nowhere to look.
    #[test]
    fn an_existing_install_gets_its_root_recorded() {
        let mut config = FoundryLocalConfig {
            managed_runtime: Some(crate::engine_config::ManagedLocalRuntimeConfig {
                kind: "hy-mt".to_string(),
                executable_path: r"D:\foundry-cache\meowcal-sub\runtime\engine\llama-server.exe"
                    .to_string(),
                model_path: r"D:\foundry-cache\meowcal-sub\models\hy-mt\model.gguf".to_string(),
                port: 11_436,
            }),
            ..FoundryLocalConfig::default()
        };

        assert!(backfill_cache_root(&mut config));
        assert_eq!(
            config.engine_cache_root.as_deref(),
            Some(r"D:\foundry-cache")
        );
    }

    // Backfilling twice must not churn the config or trigger a pointless save.
    #[test]
    fn a_recorded_root_is_not_backfilled_again() {
        let mut config = FoundryLocalConfig {
            engine_cache_root: Some(r"D:\foundry-cache".to_string()),
            ..FoundryLocalConfig::default()
        };

        assert!(!backfill_cache_root(&mut config));
    }

    // Issue #65's core case, inverted: with nothing installed anywhere, recovery
    // must decline rather than register paths that do not exist.
    #[test]
    fn nothing_is_adopted_when_no_candidate_holds_an_install() {
        let manifest = EngineManifest::shipped().expect("shipped manifest");
        let empty = std::env::temp_dir().join("meowcal-recovery-empty");
        let mut config = FoundryLocalConfig::default();

        assert!(restore_registration(&mut config, &manifest, &empty).is_none());
        assert!(config.managed_runtime.is_none());
    }
}
