use crate::engine_artifact_io::file_matches;
use crate::engine_manifest::{EngineManifest, RuntimeSpec};
use crate::hy_mt_runtime::HyMtInstallPaths;
use crate::storage_partitions::sibling_partitions;
use std::path::{Path, PathBuf};

/// An install layout `storage::import_legacy` may take verified assets from.
pub(crate) struct ImportCandidate {
    pub paths: HyMtInstallPaths,
    /// Core-owned storage. Its files may be hard-linked; a legacy application
    /// root is copied, so that application can keep changing its own files.
    pub core_owned: bool,
}

/// The current install, this client's earlier Core versions, other Core
/// partitions, then legacy roots.
pub(crate) fn import_candidates(
    paths: &HyMtInstallPaths,
    roots: &[PathBuf],
    manifest: &EngineManifest,
    runtime: &RuntimeSpec,
) -> Vec<ImportCandidate> {
    let siblings = sibling_partitions(&paths.root);
    let core_owned = std::iter::once(paths.root.clone())
        .chain(siblings.own)
        .chain(siblings.shared)
        .map(|root| (root, true));
    let legacy = roots
        .iter()
        .flat_map(|root| [root.clone(), root.join("meowcal-sub")])
        .map(|root| (root, false));
    core_owned
        .chain(legacy)
        .map(|(root, core_owned)| ImportCandidate {
            paths: HyMtInstallPaths::from_cache_root(root, manifest, runtime),
            core_owned,
        })
        .collect()
}

/// Whether `storage::import_legacy` would find a runtime archive and a model to
/// install from without a download. Sizes only: the import hashes both before
/// copying.
pub fn assets_available_offline(
    paths: &HyMtInstallPaths,
    roots: &[PathBuf],
    manifest: &EngineManifest,
) -> bool {
    let Ok(runtime) = manifest.runtime_for_current_arch() else {
        return false;
    };
    let candidates = import_candidates(paths, roots, manifest, runtime);
    let sized = |path: &Path, size: u64| path.metadata().is_ok_and(|file| file.len() == size);
    candidates
        .iter()
        .any(|candidate| sized(&candidate.paths.runtime_archive, runtime.archive.size_bytes))
        && candidates
            .iter()
            .any(|candidate| sized(&candidate.paths.model, manifest.model.artifact.size_bytes))
}

/// Stages a Core-owned source as a hard link, so the import costs no disk
/// space. Returns false when the volume cannot link, and the caller copies.
pub(crate) async fn link_candidate(source: &Path, target: &Path) -> bool {
    let Some(parent) = target.parent() else {
        return false;
    };
    if tokio::fs::create_dir_all(parent).await.is_err() {
        return false;
    }
    match tokio::fs::hard_link(source, target.with_extension("import-part")).await {
        Ok(()) => true,
        Err(error) => {
            tracing::warn!("Cannot link {}, copying instead: {error}", source.display());
            false
        }
    }
}

pub(crate) async fn promote_import(
    source: &Path,
    target: &Path,
    size: u64,
    hash: &str,
    linked: bool,
) -> Result<(), String> {
    // Selection has already verified source. Verify the staged bytes instead
    // of reading the same source twice; this also detects a changed source.
    let parent = target.parent().ok_or("CORE_IMPORT_PARENT")?;
    tokio::fs::create_dir_all(parent)
        .await
        .map_err(|error| format!("CORE_IMPORT_DIR: {error}"))?;
    let candidate = target.with_extension("import-part");
    if !linked {
        tokio::fs::copy(source, &candidate)
            .await
            .map_err(|error| format!("CORE_IMPORT_COPY: {error}"))?;
    }
    if !file_matches(&candidate, size, hash).await? {
        let _ = tokio::fs::remove_file(&candidate).await;
        return Err("CORE_IMPORT_CHANGED: source changed while copying".into());
    }
    if target.exists() {
        tokio::fs::remove_file(target)
            .await
            .map_err(|error| format!("CORE_IMPORT_REPLACE: {error}"))?;
    }
    tokio::fs::rename(candidate, target)
        .await
        .map_err(|error| format!("CORE_IMPORT_PROMOTE: {error}"))
}

#[cfg(test)]
#[path = "engine_import_sources_tests.rs"]
mod tests;
