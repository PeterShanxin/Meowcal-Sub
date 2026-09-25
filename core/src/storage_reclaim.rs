use crate::engine_artifact_io::file_matches;
use crate::engine_manifest::EngineManifest;
use crate::hy_mt_runtime::HyMtInstallPaths;
use crate::storage::Lease;
use crate::storage_partitions::sibling_partitions;
use std::path::Path;
use std::time::Duration;

/// Frees disk space held by other Core partitions of the same profile and
/// architecture, once this Core's install is verified and running. Keeps this
/// client's newest other version for rollback and removes the older ones, then
/// replaces every remaining identical model with a hard link to the current
/// one. A partition another process holds a lease on is left untouched.
pub async fn reclaim(current: &HyMtInstallPaths, manifest: &EngineManifest) {
    let Ok(runtime) = manifest.runtime_for_current_arch() else {
        return;
    };
    let root = current.root.clone();
    let siblings = tokio::task::spawn_blocking(move || sibling_partitions(&root))
        .await
        .unwrap_or_default();
    let mut own = siblings.own.into_iter();
    let previous = own.next();
    for stale in own {
        match remove_partition(&stale).await {
            Ok(true) => tracing::info!("Removed Core partition {}", stale.display()),
            Ok(false) => {}
            Err(error) => {
                tracing::warn!("Cannot remove Core partition {}: {error}", stale.display())
            }
        }
    }
    for partition in previous.into_iter().chain(siblings.shared) {
        let target = HyMtInstallPaths::from_cache_root(&partition, manifest, runtime);
        if let Err(error) = share_model(&current.model, &target, manifest).await {
            tracing::warn!(
                "Cannot share the model with {}: {error}",
                partition.display()
            );
        }
    }
}

pub fn same_file(left: &Path, right: &Path) -> std::io::Result<bool> {
    Ok(file_id(left)? == file_id(right)?)
}

async fn exclusive_lease(root: &Path) -> Result<Option<Lease>, String> {
    match Lease::acquire(root, true, Duration::ZERO).await {
        Ok(lease) => Ok(Some(lease)),
        Err(error) if error.starts_with("CORE_ASSETS_BUSY") => Ok(None),
        Err(error) => Err(error),
    }
}

async fn remove_partition(root: &Path) -> Result<bool, String> {
    let Some(lease) = exclusive_lease(root).await? else {
        return Ok(false);
    };
    let contents = root.to_owned();
    tokio::task::spawn_blocking(move || remove_contents_except_lock(&contents))
        .await
        .map_err(|error| format!("CORE_RECLAIM_TASK: {error}"))?
        .map_err(|error| format!("CORE_RECLAIM_REMOVE: {error}"))?;
    drop(lease);
    tokio::fs::remove_file(root.join("assets.lock"))
        .await
        .map_err(|error| format!("CORE_RECLAIM_LOCK: {error}"))?;
    tokio::fs::remove_dir(root)
        .await
        .map_err(|error| format!("CORE_RECLAIM_REMOVE: {error}"))?;
    // The version directory may still hold another architecture's partition.
    if let Some(version_dir) = root.parent() {
        let _ = tokio::fs::remove_dir(version_dir).await;
    }
    Ok(true)
}

fn remove_contents_except_lock(root: &Path) -> std::io::Result<()> {
    for entry in std::fs::read_dir(root)? {
        let entry = entry?;
        if entry.file_name() == "assets.lock" {
            continue;
        }
        if entry.file_type()?.is_dir() {
            std::fs::remove_dir_all(entry.path())?;
        } else {
            std::fs::remove_file(entry.path())?;
        }
    }
    Ok(())
}

async fn share_model(
    model: &Path,
    target: &HyMtInstallPaths,
    manifest: &EngineManifest,
) -> Result<(), String> {
    let artifact = &manifest.model.artifact;
    let sized = target
        .model
        .metadata()
        .is_ok_and(|metadata| metadata.is_file() && metadata.len() == artifact.size_bytes);
    if !sized
        || same_file(model, &target.model).map_err(|error| format!("CORE_SHARE_ID: {error}"))?
    {
        return Ok(());
    }
    // Hash only a partition no process is using, then keep it locked while
    // its file is swapped.
    let Some(_lease) = exclusive_lease(&target.root).await? else {
        return Ok(());
    };
    if !file_matches(&target.model, artifact.size_bytes, &artifact.sha256).await? {
        return Ok(());
    }
    let staged = target.model.with_extension("share-part");
    let _ = tokio::fs::remove_file(&staged).await;
    tokio::fs::hard_link(model, &staged)
        .await
        .map_err(|error| format!("CORE_SHARE_LINK: {error}"))?;
    if let Err(error) = tokio::fs::rename(&staged, &target.model).await {
        let _ = tokio::fs::remove_file(&staged).await;
        return Err(format!("CORE_SHARE_REPLACE: {error}"));
    }
    Ok(())
}

#[cfg(windows)]
fn file_id(path: &Path) -> std::io::Result<(u32, u64)> {
    use std::os::windows::io::AsRawHandle;
    use windows::Win32::Foundation::HANDLE;
    use windows::Win32::Storage::FileSystem::{
        GetFileInformationByHandle, BY_HANDLE_FILE_INFORMATION,
    };
    let file = std::fs::File::open(path)?;
    let mut info = BY_HANDLE_FILE_INFORMATION::default();
    // SAFETY: the handle belongs to `file`, which outlives the call.
    unsafe { GetFileInformationByHandle(HANDLE(file.as_raw_handle()), &mut info) }
        .map_err(std::io::Error::other)?;
    Ok((
        info.dwVolumeSerialNumber,
        (u64::from(info.nFileIndexHigh) << 32) | u64::from(info.nFileIndexLow),
    ))
}

#[cfg(not(windows))]
fn file_id(_path: &Path) -> std::io::Result<(u32, u64)> {
    Err(std::io::Error::new(
        std::io::ErrorKind::Unsupported,
        "Windows is required",
    ))
}

#[cfg(test)]
#[path = "storage_reclaim_tests.rs"]
mod tests;
