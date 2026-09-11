use crate::engine_artifact_io::file_matches;
use crate::engine_manifest::EngineManifest;
use crate::hy_mt_runtime::HyMtInstallPaths;
use crate::protocol::CORE_VERSION;
use fs2::FileExt;
use std::fs::{File, OpenOptions};
use std::path::{Component, Path, PathBuf};
use std::time::Duration;

pub fn resolve_root(profile: &str, base: Option<&Path>) -> Result<PathBuf, String> {
    if !matches!(profile, "production" | "development") {
        return Err("INVALID_PROFILE".into());
    }
    let base = match base {
        Some(path) => path.to_owned(),
        None => PathBuf::from(std::env::var_os("LOCALAPPDATA").ok_or("CORE_STORAGE_UNAVAILABLE")?)
            .join("Meowcal")
            .join("Core"),
    };
    validate_absolute(&base)?;
    Ok(base
        .join(profile)
        .join(CORE_VERSION)
        .join(std::env::consts::ARCH))
}

pub fn validate_absolute(path: &Path) -> Result<(), String> {
    if !path.is_absolute()
        || path
            .components()
            .any(|part| matches!(part, Component::ParentDir))
    {
        return Err("CORE_STORAGE_PATH: expected an absolute path without parent traversal".into());
    }
    Ok(())
}

/// Held for every runtime's full lifetime; installation requires exclusive access.
pub struct Lease(File);

impl Lease {
    pub async fn acquire(root: &Path, exclusive: bool, budget: Duration) -> Result<Self, String> {
        tokio::fs::create_dir_all(root)
            .await
            .map_err(|error| format!("CORE_STORAGE_CREATE: {error}"))?;
        let path = root.join("assets.lock");
        let file = tokio::task::spawn_blocking(move || {
            OpenOptions::new()
                .read(true)
                .write(true)
                .create(true)
                .truncate(false)
                .open(path)
        })
        .await
        .map_err(|error| format!("CORE_LOCK_TASK: {error}"))?
        .map_err(|error| format!("CORE_LOCK_OPEN: {error}"))?;
        let deadline = tokio::time::Instant::now() + budget;
        loop {
            let result = if exclusive {
                FileExt::try_lock_exclusive(&file)
            } else {
                FileExt::try_lock_shared(&file)
            };
            match result {
                Ok(()) => return Ok(Self(file)),
                Err(error)
                    if error.kind() == std::io::ErrorKind::WouldBlock
                        || error.raw_os_error() == fs2::lock_contended_error().raw_os_error() => {}
                Err(error) => return Err(format!("CORE_LOCK_FAILED: {error}")),
            }
            if tokio::time::Instant::now() >= deadline {
                return Err(
                    "CORE_ASSETS_BUSY: another Core process is using the installation".into(),
                );
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }
}

impl Drop for Lease {
    fn drop(&mut self) {
        let _ = FileExt::unlock(&self.0);
    }
}

pub async fn verify(paths: &HyMtInstallPaths, manifest: &EngineManifest) -> Result<(), String> {
    verify_runtime(paths, manifest).await?;
    if !file_matches(
        &paths.model,
        manifest.model.artifact.size_bytes,
        &manifest.model.artifact.sha256,
    )
    .await?
    {
        return Err("CORE_ASSETS_UNVERIFIED: install or repair is required".into());
    }
    Ok(())
}

pub async fn verify_runtime(
    paths: &HyMtInstallPaths,
    manifest: &EngineManifest,
) -> Result<(), String> {
    let runtime = manifest
        .runtime_for_current_arch()
        .map_err(|error| error.to_string())?;
    for (path, size, hash) in [
        (
            &paths.runtime_archive,
            runtime.archive.size_bytes,
            runtime.archive.sha256.as_str(),
        ),
        (
            &paths.executable,
            runtime.executable.size_bytes,
            runtime.executable.sha256.as_str(),
        ),
    ] {
        if !file_matches(path, size, hash).await? {
            return Err("CORE_ASSETS_UNVERIFIED: install or repair is required".into());
        }
    }
    let archive = paths.runtime_archive.clone();
    let directory = paths.runtime_dir.clone();
    tokio::task::spawn_blocking(move || verify_runtime_tree(&archive, &directory))
        .await
        .map_err(|error| format!("CORE_VERIFY_TASK: {error}"))?
}

fn verify_runtime_tree(archive: &Path, directory: &Path) -> Result<(), String> {
    let file = File::open(archive).map_err(|error| format!("CORE_ARCHIVE_OPEN: {error}"))?;
    let mut zip =
        zip::ZipArchive::new(file).map_err(|error| format!("CORE_ARCHIVE_READ: {error}"))?;
    let mut expected_files = std::collections::HashSet::new();
    let mut archive_names = std::collections::HashSet::new();
    for index in 0..zip.len() {
        let mut entry = zip
            .by_index(index)
            .map_err(|error| format!("CORE_ARCHIVE_ENTRY: {error}"))?;
        let relative = entry
            .enclosed_name()
            .ok_or("CORE_ARCHIVE_UNSAFE_PATH")?
            .to_owned();
        let name = relative
            .components()
            .filter_map(|component| match component {
                Component::Normal(part) => Some(part.to_string_lossy().to_lowercase()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("/");
        if !archive_names.insert(name)
            || entry
                .unix_mode()
                .is_some_and(|mode| mode & 0o170000 == 0o120000)
        {
            return Err("CORE_ARCHIVE_UNSAFE_PATH".into());
        }
        if entry.is_dir() {
            continue;
        }
        let path = directory.join(relative);
        expected_files.insert(path.clone());
        let mut actual =
            File::open(path).map_err(|_| "CORE_RUNTIME_TREE_UNVERIFIED".to_string())?;
        if actual.metadata().map_err(|error| error.to_string())?.len() != entry.size() {
            return Err("CORE_RUNTIME_TREE_UNVERIFIED".into());
        }
        if crate::sha256::digest_reader(&mut entry)? != crate::sha256::digest_reader(&mut actual)? {
            return Err("CORE_RUNTIME_TREE_UNVERIFIED".into());
        }
    }
    verify_tree_members(directory, &expected_files)?;
    Ok(())
}

fn verify_tree_members(
    directory: &Path,
    expected: &std::collections::HashSet<PathBuf>,
) -> Result<(), String> {
    let metadata = std::fs::symlink_metadata(directory).map_err(|error| error.to_string())?;
    if metadata.file_type().is_symlink() {
        return Err("CORE_RUNTIME_TREE_UNVERIFIED".into());
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if metadata.file_attributes() & 0x400 != 0 {
            return Err("CORE_RUNTIME_TREE_UNVERIFIED".into());
        }
    }
    for entry in
        std::fs::read_dir(directory).map_err(|error| format!("CORE_RUNTIME_TREE_READ: {error}"))?
    {
        let entry = entry.map_err(|error| error.to_string())?;
        let metadata =
            std::fs::symlink_metadata(entry.path()).map_err(|error| error.to_string())?;
        if metadata.file_type().is_symlink() {
            return Err("CORE_RUNTIME_TREE_UNVERIFIED".into());
        }
        #[cfg(windows)]
        {
            use std::os::windows::fs::MetadataExt;
            if metadata.file_attributes() & 0x400 != 0 {
                return Err("CORE_RUNTIME_TREE_UNVERIFIED".into());
            }
        }
        if metadata.is_dir() {
            verify_tree_members(&entry.path(), expected)?;
        } else if !metadata.is_file() || !expected.contains(&entry.path()) {
            return Err("CORE_RUNTIME_TREE_UNVERIFIED".into());
        }
    }
    Ok(())
}

/// Import only authenticated archives and models. DLL directories are never adopted.
/// The caller holds the exclusive asset lease, including during staging cleanup.
pub async fn import_legacy(
    paths: &HyMtInstallPaths,
    roots: &[PathBuf],
    manifest: &EngineManifest,
    require_complete: bool,
) -> Result<(), String> {
    crate::engine_install_transaction::recover_pending_asset(&paths.runtime_dir).await?;
    crate::engine_install_transaction::recover_pending_asset(&paths.model).await?;
    cleanup_staging(paths).await?;
    let runtime = manifest
        .runtime_for_current_arch()
        .map_err(|error| error.to_string())?;
    let mut candidates = vec![paths.clone()];
    for root in roots {
        validate_absolute(root)?;
        for candidate_root in [root.clone(), root.join("meowcal-sub")] {
            candidates.push(HyMtInstallPaths::from_cache_root(
                candidate_root,
                manifest,
                runtime,
            ));
        }
    }
    let mut archive = None;
    let mut model = None;
    for candidate in candidates {
        if archive.is_none()
            && file_matches(
                &candidate.runtime_archive,
                runtime.archive.size_bytes,
                &runtime.archive.sha256,
            )
            .await?
        {
            archive = Some(candidate.runtime_archive);
        }
        if model.is_none()
            && file_matches(
                &candidate.model,
                manifest.model.artifact.size_bytes,
                &manifest.model.artifact.sha256,
            )
            .await?
        {
            model = Some(candidate.model);
        }
        if archive.is_some() && model.is_some() {
            break;
        }
    }
    if require_complete && (archive.is_none() || model.is_none()) {
        return Err("CORE_ASSETS_UNVERIFIED: verified local archive and model are required; run install or repair".into());
    }
    let copying = archive
        .as_ref()
        .is_some_and(|source| source != &paths.runtime_archive)
        || model.as_ref().is_some_and(|source| source != &paths.model);
    if copying {
        crate::engine_preflight::run(&paths.root, &manifest.requirements, true).await?;
    }
    if let Some(source) = archive {
        copy_verified_source(
            &source,
            &paths.runtime_archive,
            runtime.archive.size_bytes,
            &runtime.archive.sha256,
        )
        .await?;
    }
    if let Some(source) = model {
        copy_verified_source(
            &source,
            &paths.model,
            manifest.model.artifact.size_bytes,
            &manifest.model.artifact.sha256,
        )
        .await?;
    }
    Ok(())
}

async fn cleanup_staging(paths: &HyMtInstallPaths) -> Result<(), String> {
    let mut staging = vec![
        paths.runtime_archive.with_extension("import-part"),
        paths.model.with_extension("import-part"),
    ];
    for asset in [&paths.runtime_dir, &paths.model] {
        let mut candidate = asset.as_os_str().to_owned();
        candidate.push(".candidate");
        staging.push(PathBuf::from(candidate));
    }
    for path in staging {
        if path == paths.root || !path.starts_with(&paths.root) {
            return Err("CORE_STAGING_PATH_INVALID".into());
        }
        crate::engine_install_transaction::reset_candidate(&path).await?;
    }
    Ok(())
}

async fn copy_verified_source(
    source: &Path,
    target: &Path,
    size: u64,
    hash: &str,
) -> Result<(), String> {
    // Selection has already verified source. Verify the copied bytes instead
    // of reading the same source twice; this also detects a changed source.
    if source == target {
        return Ok(());
    }
    let parent = target.parent().ok_or("CORE_IMPORT_PARENT")?;
    tokio::fs::create_dir_all(parent)
        .await
        .map_err(|error| format!("CORE_IMPORT_DIR: {error}"))?;
    let candidate = target.with_extension("import-part");
    tokio::fs::copy(source, &candidate)
        .await
        .map_err(|error| format!("CORE_IMPORT_COPY: {error}"))?;
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
#[path = "storage_tests.rs"]
mod tests;
