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

#[cfg(test)]
#[path = "engine_import_sources_tests.rs"]
mod tests;
