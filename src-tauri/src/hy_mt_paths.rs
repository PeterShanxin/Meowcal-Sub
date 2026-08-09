// =============================================================================
// HY_MT_PATHS.RS - where the managed HY-MT engine lives on disk
// =============================================================================
// Split from `hy_mt_runtime` when the launch policy grew the runtime module
// past the 400-line ceiling; layout rules stay here, process lifecycle stays
// there. Re-exported from `hy_mt_runtime`, so existing callers are unchanged.
// =============================================================================

use crate::config::ManagedLocalRuntimeConfig;
use crate::engine_manifest::{EngineManifest, RuntimeSpec};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct HyMtInstallPaths {
    pub root: PathBuf,
    pub runtime_dir: PathBuf,
    pub runtime_archive: PathBuf,
    pub executable: PathBuf,
    pub model_dir: PathBuf,
    pub model: PathBuf,
}

impl HyMtInstallPaths {
    pub fn from_cache_root(
        cache_root: impl AsRef<Path>,
        manifest: &EngineManifest,
        runtime: &RuntimeSpec,
    ) -> Self {
        let root = cache_root.as_ref().join("meowcal-sub");
        let runtime_dir = root.join("runtime").join(&runtime.install_directory);
        let model_dir = root.join("models").join(&manifest.model.install_directory);
        Self {
            runtime_archive: root.join("runtime").join(&runtime.archive.file_name),
            executable: runtime_dir.join(&runtime.executable.relative_path),
            model: model_dir.join(&manifest.model.artifact.file_name),
            root,
            runtime_dir,
            model_dir,
        }
    }

    pub fn is_complete(&self, manifest: &EngineManifest, runtime: &RuntimeSpec) -> bool {
        self.executable
            .metadata()
            .map(|metadata| metadata.len() == runtime.executable.size_bytes)
            .unwrap_or(false)
            && self
                .model
                .metadata()
                .map(|metadata| metadata.len() == manifest.model.artifact.size_bytes)
                .unwrap_or(false)
    }

    pub fn managed_config(&self, manifest: &EngineManifest) -> ManagedLocalRuntimeConfig {
        ManagedLocalRuntimeConfig {
            kind: "hy-mt".to_string(),
            executable_path: self.executable.to_string_lossy().to_string(),
            model_path: self.model.to_string_lossy().to_string(),
            port: manifest.launch.preferred_port,
        }
    }
}
