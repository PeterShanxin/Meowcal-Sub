use crate::engine_manifest::{EngineManifest, RuntimeSpec};
use crate::hy_mt_runtime::HyMtInstallPaths;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::io::Read;
use std::path::{Component, Path, PathBuf};
use tokio::fs;

const STATE_FILE: &str = "install-state.v1.json";
const STATE_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstalledEngine {
    pub engine_version: String,
    pub runtime_id: String,
    pub architecture: String,
    pub runtime_dir: PathBuf,
    pub runtime_archive: PathBuf,
    pub executable: PathBuf,
    pub executable_size: u64,
    pub executable_sha256: String,
    pub model_dir: PathBuf,
    pub model: PathBuf,
    pub model_size: u64,
    pub model_sha256: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct InstallState {
    schema_version: u32,
    active: Option<InstalledEngine>,
    last_known_good: Option<InstalledEngine>,
}

pub struct Promotion {
    assets: Vec<PromotedAsset>,
}

struct PromotedAsset {
    final_path: PathBuf,
    backup_path: PathBuf,
    had_previous: bool,
}

impl InstalledEngine {
    pub fn from_install(
        paths: &HyMtInstallPaths,
        manifest: &EngineManifest,
        runtime: &RuntimeSpec,
    ) -> Result<Self, String> {
        Ok(Self {
            engine_version: manifest.engine_version.clone(),
            runtime_id: runtime.id.clone(),
            architecture: runtime.architecture.as_str().to_string(),
            runtime_dir: relative_to_root(&paths.root, &paths.runtime_dir)?,
            runtime_archive: relative_to_root(&paths.root, &paths.runtime_archive)?,
            executable: relative_to_root(&paths.root, &paths.executable)?,
            executable_size: runtime.executable.size_bytes,
            executable_sha256: runtime.executable.sha256.clone(),
            model_dir: relative_to_root(&paths.root, &paths.model_dir)?,
            model: relative_to_root(&paths.root, &paths.model)?,
            model_size: manifest.model.artifact.size_bytes,
            model_sha256: manifest.model.artifact.sha256.clone(),
        })
    }

    pub fn paths(&self, root: &Path) -> Result<HyMtInstallPaths, String> {
        for path in [
            &self.runtime_dir,
            &self.runtime_archive,
            &self.executable,
            &self.model_dir,
            &self.model,
        ] {
            validate_relative(path)?;
        }
        Ok(HyMtInstallPaths {
            root: root.to_path_buf(),
            runtime_dir: root.join(&self.runtime_dir),
            runtime_archive: root.join(&self.runtime_archive),
            executable: root.join(&self.executable),
            model_dir: root.join(&self.model_dir),
            model: root.join(&self.model),
        })
    }
}

pub async fn promote_assets(assets: &[(PathBuf, PathBuf)]) -> Result<Promotion, String> {
    let mut promoted = Vec::new();
    for (candidate, final_path) in assets {
        if let Err(error) = recover_interrupted_promotion(final_path).await {
            rollback_assets(&promoted).await;
            return Err(error);
        }
        let backup = backup_path(final_path);
        let had_previous = final_path.exists();
        if had_previous {
            fs::rename(final_path, &backup)
                .await
                .map_err(|error| format!("ENGINE_PROMOTE_BACKUP: {error}"))?;
        }
        if let Err(error) = fs::rename(candidate, final_path).await {
            if had_previous {
                let _ = fs::rename(&backup, final_path).await;
            }
            rollback_assets(&promoted).await;
            return Err(format!("ENGINE_PROMOTE_CANDIDATE: {error}"));
        }
        promoted.push(PromotedAsset {
            final_path: final_path.clone(),
            backup_path: backup,
            had_previous,
        });
    }
    Ok(Promotion { assets: promoted })
}

pub async fn reset_candidate(path: &Path) -> Result<(), String> {
    remove_path_if_present(path).await
}

pub async fn recover_pending_asset(path: &Path) -> Result<(), String> {
    recover_interrupted_promotion(path).await
}

impl Promotion {
    pub async fn commit(self) -> Result<(), String> {
        for asset in self.assets {
            remove_path_if_present(&asset.backup_path).await?;
        }
        Ok(())
    }

    pub async fn rollback(self) {
        rollback_assets(&self.assets).await;
    }
}

pub async fn record_active(root: &Path, record: InstalledEngine) -> Result<(), String> {
    let mut state = load_state(root).await.unwrap_or_default();
    state.schema_version = STATE_SCHEMA_VERSION;
    if state.active.as_ref() != Some(&record) {
        if let Some(active) = state.active.take() {
            if record_is_usable(root, &active).await {
                state.last_known_good = Some(active);
            }
        }
    }
    state.active = Some(record);
    write_state(root, &state).await
}

pub async fn recover_active(root: &Path) -> Option<HyMtInstallPaths> {
    let mut state = load_state(root).await.ok()?;
    if let Some(active) = state.active.as_ref() {
        if record_is_usable(root, active).await {
            return active.paths(root).ok();
        }
    }
    let fallback = state.last_known_good.clone()?;
    if !record_is_usable(root, &fallback).await {
        return None;
    }
    state.active = Some(fallback.clone());
    state.last_known_good = None;
    let _ = write_state(root, &state).await;
    fallback.paths(root).ok()
}

async fn recover_interrupted_promotion(final_path: &Path) -> Result<(), String> {
    let backup = backup_path(final_path);
    if !backup.exists() {
        return Ok(());
    }
    if final_path.exists() {
        remove_path_if_present(final_path).await?;
    }
    fs::rename(&backup, final_path)
        .await
        .map_err(|error| format!("ENGINE_ROLLBACK_RESTORE: {error}"))
}

async fn rollback_assets(assets: &[PromotedAsset]) {
    for asset in assets.iter().rev() {
        let _ = remove_path_if_present(&asset.final_path).await;
        if asset.had_previous {
            let _ = fs::rename(&asset.backup_path, &asset.final_path).await;
        }
    }
}

async fn remove_path_if_present(path: &Path) -> Result<(), String> {
    let Ok(metadata) = fs::metadata(path).await else {
        return Ok(());
    };
    if metadata.is_dir() {
        fs::remove_dir_all(path)
            .await
            .map_err(|error| format!("ENGINE_REMOVE_STALE_DIR: {error}"))
    } else {
        fs::remove_file(path)
            .await
            .map_err(|error| format!("ENGINE_REMOVE_STALE_FILE: {error}"))
    }
}

async fn load_state(root: &Path) -> Result<InstallState, String> {
    let state_path = root.join(STATE_FILE);
    let backup = backup_path(&state_path);
    let mut last_error = None;
    for candidate in [&state_path, &backup] {
        if let Ok(bytes) = fs::read(candidate).await {
            match serde_json::from_slice::<InstallState>(&bytes) {
                Ok(state) if state.schema_version == STATE_SCHEMA_VERSION => return Ok(state),
                Ok(_) => last_error = Some("ENGINE_STATE_SCHEMA".to_string()),
                Err(error) => last_error = Some(format!("ENGINE_STATE_PARSE: {error}")),
            }
        }
    }
    last_error.map_or_else(|| Ok(InstallState::default()), Err)
}

async fn write_state(root: &Path, state: &InstallState) -> Result<(), String> {
    fs::create_dir_all(root)
        .await
        .map_err(|error| format!("ENGINE_STATE_DIR: {error}"))?;
    let target = root.join(STATE_FILE);
    let part = root.join(format!("{STATE_FILE}.part"));
    let backup = backup_path(&target);
    let bytes = serde_json::to_vec_pretty(state)
        .map_err(|error| format!("ENGINE_STATE_SERIALIZE: {error}"))?;
    fs::write(&part, bytes)
        .await
        .map_err(|error| format!("ENGINE_STATE_WRITE: {error}"))?;
    recover_interrupted_promotion(&target).await?;
    let had_previous = target.exists();
    if had_previous {
        fs::rename(&target, &backup)
            .await
            .map_err(|error| format!("ENGINE_STATE_BACKUP: {error}"))?;
    }
    if let Err(error) = fs::rename(&part, &target).await {
        if had_previous {
            let _ = fs::rename(&backup, &target).await;
        }
        return Err(format!("ENGINE_STATE_FINALIZE: {error}"));
    }
    remove_path_if_present(&backup).await
}

async fn record_is_usable(root: &Path, record: &InstalledEngine) -> bool {
    let Ok(paths) = record.paths(root) else {
        return false;
    };
    file_matches(
        &paths.executable,
        record.executable_size,
        &record.executable_sha256,
    )
    .await
        && file_matches(&paths.model, record.model_size, &record.model_sha256).await
}

async fn file_matches(path: &Path, size: u64, expected_hash: &str) -> bool {
    if path.metadata().map(|meta| meta.len()).ok() != Some(size) {
        return false;
    }
    let path = path.to_path_buf();
    tauri::async_runtime::spawn_blocking(move || sha256_file(&path))
        .await
        .ok()
        .and_then(Result::ok)
        .is_some_and(|hash| hash.eq_ignore_ascii_case(expected_hash))
}

fn sha256_file(path: &Path) -> Result<String, String> {
    let mut file = std::fs::File::open(path).map_err(|error| error.to_string())?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 1024 * 1024];
    loop {
        let count = file.read(&mut buffer).map_err(|error| error.to_string())?;
        if count == 0 {
            break;
        }
        hasher.update(&buffer[..count]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn relative_to_root(root: &Path, path: &Path) -> Result<PathBuf, String> {
    let relative = path
        .strip_prefix(root)
        .map_err(|_| "ENGINE_STATE_PATH_OUTSIDE_ROOT".to_string())?
        .to_path_buf();
    validate_relative(&relative)?;
    Ok(relative)
}

fn validate_relative(path: &Path) -> Result<(), String> {
    if path.as_os_str().is_empty()
        || path.is_absolute()
        || path.components().any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
    {
        return Err("ENGINE_STATE_PATH_INVALID".to_string());
    }
    Ok(())
}

fn backup_path(path: &Path) -> PathBuf {
    let mut name = path.as_os_str().to_os_string();
    name.push(".rollback");
    PathBuf::from(name)
}

#[cfg(test)]
#[path = "engine_install_transaction_tests.rs"]
mod tests;
