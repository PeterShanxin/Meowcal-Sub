use crate::config::FoundryLocalConfig;
use crate::engine_manifest::EngineManifest;
use crate::hy_mt_runtime::HyMtInstallPaths;
use std::path::{Path, PathBuf};

pub fn candidate_roots(recorded: Option<&str>, default_root: &Path) -> Vec<PathBuf> {
    let mut roots = Vec::new();
    if let Some(recorded) = recorded.map(str::trim).filter(|root| !root.is_empty()) {
        roots.push(PathBuf::from(recorded));
    }
    let default_root = default_root.to_path_buf();
    if !roots.contains(&default_root) {
        roots.push(default_root);
    }
    roots
}

pub fn install_cache_root(config: &FoundryLocalConfig) -> Option<String> {
    config
        .managed_cache_root()
        .map(|path| path.to_string_lossy().to_string())
        .filter(|root| volume_exists(Path::new(root)))
        .or_else(|| {
            config
                .engine_cache_root
                .as_deref()
                .map(str::trim)
                .filter(|root| !root.is_empty())
                .map(str::to_string)
                .filter(|root| volume_exists(Path::new(root)))
        })
}

fn volume_exists(root: &Path) -> bool {
    match root.components().next() {
        Some(std::path::Component::Prefix(prefix)) => {
            PathBuf::from(format!("{}\\", prefix.as_os_str().to_string_lossy())).is_dir()
        }
        _ => true,
    }
}

pub fn cache_root_of(paths: &HyMtInstallPaths) -> Option<String> {
    crate::engine_config::core_storage_base(&paths.root)
        .or_else(|| paths.root.parent())
        .map(|root| root.to_string_lossy().to_string())
}

pub fn configure_core(
    app: &tauri::AppHandle,
    config: &mut crate::config::AppConfig,
) -> Result<(), String> {
    use tauri::Manager;

    let default_root = app
        .path()
        .app_cache_dir()
        .map_err(|error| format!("ENGINE_CACHE_PATH: {error}"))?;
    let mut legacy_roots = candidate_roots(
        config
            .translation
            .foundry_local
            .engine_cache_root
            .as_deref(),
        &default_root,
    );
    if let Some(root) = config.translation.foundry_local.managed_cache_root() {
        if !legacy_roots.contains(&root) {
            legacy_roots.insert(0, root);
        }
    }
    let selected = install_cache_root(&config.translation.foundry_local).map(PathBuf::from);
    crate::core_client::configure_storage(
        selected.filter(|root| root != &default_root),
        legacy_roots,
    )?;

    let status = crate::core_client::status_blocking()?;
    if status.installed {
        persist_core_status(app, config, status)?;
    }
    Ok(())
}

fn persist_core_status(
    app: &tauri::AppHandle,
    config: &mut crate::config::AppConfig,
    status: crate::core_client::CoreStatus,
) -> Result<(), String> {
    let runtime = status
        .managed_config
        .ok_or_else(|| "CORE_MANAGED_CONFIG_MISSING".to_string())?;
    let engine = &mut config.translation.foundry_local;
    let root = crate::engine_config::core_storage_base(&status.storage_root)
        .map(|path| path.to_string_lossy().to_string());
    let changed = engine.managed_runtime.as_ref() != Some(&runtime)
        || engine.model.as_deref() != Some(status.model.as_str())
        || engine.engine_cache_root != root;
    engine.endpoint_url = Some(crate::hy_mt_runtime::endpoint_url(&runtime));
    engine.model = Some(status.model);
    engine.managed_runtime = Some(runtime);
    engine.engine_cache_root = root;
    if changed {
        crate::config_save::save_config(app, config)?;
    }
    Ok(())
}

pub fn backfill_cache_root(config: &mut FoundryLocalConfig) -> bool {
    if config
        .engine_cache_root
        .as_deref()
        .is_some_and(|root| !root.trim().is_empty())
    {
        return false;
    }
    let Some(root) = config.managed_cache_root() else {
        return false;
    };
    config.engine_cache_root = Some(root.to_string_lossy().to_string());
    true
}

pub fn load_with_engine(app: &tauri::AppHandle) -> crate::config::AppConfig {
    use tauri::Manager;

    let mut config = crate::config_store::load_config(app);
    let default_root = match app.path().app_cache_dir() {
        Ok(root) => root,
        Err(error) => {
            tracing::error!("Cannot resolve the legacy engine cache: {error}");
            return config;
        }
    };
    let changed = if config.translation.foundry_local.managed_runtime.is_some() {
        backfill_cache_root(&mut config.translation.foundry_local)
    } else {
        add_migration_record(&mut config.translation.foundry_local, &default_root)
    };
    if changed {
        if let Err(error) = crate::config_save::save_config(app, &config) {
            tracing::error!("Could not persist the engine migration record: {error}");
        }
    }
    config
}

fn add_migration_record(config: &mut FoundryLocalConfig, default_root: &Path) -> bool {
    let manifest = match EngineManifest::shipped() {
        Ok(manifest) => manifest,
        Err(error) => {
            tracing::error!("Cannot read the shared engine manifest: {error}");
            return false;
        }
    };
    let runtime = match manifest.runtime_for_current_arch() {
        Ok(runtime) => runtime,
        Err(error) => {
            tracing::error!("Cannot select the shared engine runtime: {error}");
            return false;
        }
    };
    let recorded = config
        .engine_cache_root
        .as_deref()
        .map(str::trim)
        .filter(|root| !root.is_empty());
    let root = candidate_roots(recorded, default_root)
        .into_iter()
        .find(|root| {
            if recorded.is_some_and(|recorded| Path::new(recorded) == root) {
                return true;
            }
            let paths =
                HyMtInstallPaths::from_cache_root(root.join("meowcal-sub"), &manifest, runtime);
            paths.executable.is_file() || paths.model.is_file() || paths.runtime_archive.is_file()
        });
    let Some(root) = root else { return false };
    let paths = HyMtInstallPaths::from_cache_root(root.join("meowcal-sub"), &manifest, runtime);
    let migration = paths.managed_config(&manifest);
    config.endpoint_url = Some(crate::hy_mt_runtime::endpoint_url(&migration));
    config.model = Some(manifest.model.id.clone());
    config.managed_runtime = Some(migration);
    config.engine_cache_root = Some(root.to_string_lossy().to_string());
    true
}

#[cfg(test)]
#[path = "engine_recovery_tests.rs"]
mod tests;
