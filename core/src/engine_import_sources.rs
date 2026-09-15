use crate::engine_manifest::{EngineManifest, RuntimeSpec};
use crate::hy_mt_runtime::HyMtInstallPaths;
use std::path::{Path, PathBuf};

/// Install layouts `storage::import_legacy` may take verified assets from: the
/// current install, earlier Core versions' partitions beside it, then legacy
/// roots.
pub(crate) fn import_candidates(
    paths: &HyMtInstallPaths,
    roots: &[PathBuf],
    manifest: &EngineManifest,
    runtime: &RuntimeSpec,
) -> Vec<HyMtInstallPaths> {
    let mut candidates = vec![paths.clone()];
    for partition in other_core_partitions(&paths.root) {
        candidates.push(HyMtInstallPaths::from_cache_root(
            partition, manifest, runtime,
        ));
    }
    for root in roots {
        for candidate_root in [root.clone(), root.join("meowcal-sub")] {
            candidates.push(HyMtInstallPaths::from_cache_root(
                candidate_root,
                manifest,
                runtime,
            ));
        }
    }
    candidates
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
        .any(|candidate| sized(&candidate.runtime_archive, runtime.archive.size_bytes))
        && candidates
            .iter()
            .any(|candidate| sized(&candidate.model, manifest.model.artifact.size_bytes))
}

/// Other Core versions' partitions for the same profile and architecture,
/// newest first. `storage::resolve_root` lays storage out as
/// `<base>/<profile>/<version>/<architecture>`.
fn other_core_partitions(root: &Path) -> Vec<PathBuf> {
    let (Some(architecture), Some(version_dir)) = (root.file_name(), root.parent()) else {
        return Vec::new();
    };
    let (Some(current), Some(profile_dir)) = (version_dir.file_name(), version_dir.parent()) else {
        return Vec::new();
    };
    let Ok(entries) = std::fs::read_dir(profile_dir) else {
        return Vec::new();
    };
    let mut partitions: Vec<(Vec<u64>, PathBuf)> = entries
        .filter_map(Result::ok)
        .filter(|entry| entry.file_name() != current)
        .filter_map(|entry| {
            let version = entry
                .file_name()
                .to_str()?
                .split('.')
                .map(|part| part.parse::<u64>().ok())
                .collect::<Option<Vec<_>>>()?;
            let partition = entry.path().join(architecture);
            (version.len() == 3 && partition.is_dir()).then_some((version, partition))
        })
        .collect();
    partitions.sort_by(|left, right| right.0.cmp(&left.0));
    partitions
        .into_iter()
        .map(|(_, partition)| partition)
        .collect()
}

#[cfg(test)]
#[path = "engine_import_sources_tests.rs"]
mod tests;
