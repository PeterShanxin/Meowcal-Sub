use crate::engine_manifest::{DownloadArtifact, InstalledExecutable};
use crate::sha256::digest_file_hex;
use reqwest::Client;
use std::path::{Component, Path};
use std::time::Duration;
use tokio::fs;
use tokio::io::AsyncWriteExt;

pub async fn file_matches(
    path: &Path,
    expected_size: u64,
    expected_hash: &str,
) -> Result<bool, String> {
    if path
        .metadata()
        .map(|metadata| metadata.len() != expected_size)
        .unwrap_or(true)
    {
        return Ok(false);
    }
    let owned_path = path.to_path_buf();
    let digest = tokio::task::spawn_blocking(move || digest_file_hex(&owned_path))
        .await
        .map_err(|error| format!("ENGINE_VERIFY_TASK: {error}"))??;
    Ok(digest.eq_ignore_ascii_case(expected_hash))
}

/// `file_matches` for synchronous recovery and legacy-adoption callers.
pub fn file_matches_blocking(path: &Path, expected_size: u64, expected_hash: &str) -> bool {
    if path
        .metadata()
        .map(|metadata| metadata.len() != expected_size)
        .unwrap_or(true)
    {
        return false;
    }
    digest_file_hex(path)
        .map(|digest| digest.eq_ignore_ascii_case(expected_hash))
        .unwrap_or(false)
}

pub async fn verify_download(
    path: &Path,
    artifact: &DownloadArtifact,
    label: &str,
) -> Result<(), String> {
    verify_file(path, artifact.size_bytes, &artifact.sha256, label).await
}

pub async fn verify_executable(
    path: &Path,
    executable: &InstalledExecutable,
) -> Result<(), String> {
    verify_file(
        path,
        executable.size_bytes,
        &executable.sha256,
        "RUNTIME_EXECUTABLE",
    )
    .await
}

async fn verify_file(
    path: &Path,
    expected_size: u64,
    expected_hash: &str,
    artifact: &str,
) -> Result<(), String> {
    if file_matches(path, expected_size, expected_hash).await? {
        Ok(())
    } else {
        Err(format!(
            "ENGINE_{artifact}_INTEGRITY_MISMATCH: retry Install / Repair"
        ))
    }
}

pub async fn download_file(
    progress: &(dyn Fn(String) + Send + Sync),
    url: &str,
    target: &Path,
    expected_size: Option<u64>,
    label: &str,
) -> Result<(), String> {
    let part = target.with_extension("download.part");
    let mut existing = fs::metadata(&part)
        .await
        .map(|metadata| metadata.len())
        .unwrap_or(0);
    if expected_size.is_some_and(|size| existing > size) {
        fs::remove_file(&part)
            .await
            .map_err(|error| format!("ENGINE_PARTIAL_RESET: {error}"))?;
        existing = 0;
    }

    let client = Client::builder()
        .user_agent(concat!("Meowcal-Core/", env!("CARGO_PKG_VERSION")))
        .timeout(Duration::from_secs(600))
        .build()
        .map_err(|error| format!("ENGINE_DOWNLOAD_CLIENT: {error}"))?;
    let mut request = client.get(url);
    if existing > 0 {
        request = request.header(reqwest::header::RANGE, format!("bytes={existing}-"));
    }
    let mut response = request
        .send()
        .await
        .map_err(|error| format!("ENGINE_DOWNLOAD_REQUEST: {label}: {error}"))?;
    if !response.status().is_success() {
        return Err(format!(
            "ENGINE_DOWNLOAD_HTTP: {label}: {}",
            response.status()
        ));
    }

    let resumed = existing > 0 && response.status() == reqwest::StatusCode::PARTIAL_CONTENT;
    if !resumed {
        existing = 0;
    }
    let mut file = fs::OpenOptions::new()
        .create(true)
        .write(true)
        .append(resumed)
        .truncate(!resumed)
        .open(&part)
        .await
        .map_err(|error| format!("ENGINE_PARTIAL_OPEN: {error}"))?;
    let total = expected_size.or_else(|| {
        response
            .content_length()
            .map(|remaining| remaining.saturating_add(existing))
    });
    let mut written = existing;
    let mut last_bucket = u64::MAX;
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|error| format!("ENGINE_DOWNLOAD_STREAM: {label}: {error}"))?
    {
        file.write_all(&chunk)
            .await
            .map_err(|error| format!("ENGINE_DOWNLOAD_WRITE: {label}: {error}"))?;
        written = written.saturating_add(chunk.len() as u64);
        if let Some(total) = total.filter(|total| *total > 0) {
            let percent = written.saturating_mul(100) / total;
            if percent / 5 != last_bucket {
                progress(format!("{label}: {}%", percent.min(100)));
                last_bucket = percent / 5;
            }
        }
    }
    file.flush()
        .await
        .map_err(|error| format!("ENGINE_DOWNLOAD_FLUSH: {label}: {error}"))?;
    drop(file);

    if expected_size.is_some_and(|size| written != size) {
        return Err(format!(
            "ENGINE_DOWNLOAD_INCOMPLETE: {label}: expected {} bytes, received {written}",
            expected_size.unwrap_or_default()
        ));
    }
    if target.exists() {
        fs::remove_file(target)
            .await
            .map_err(|error| format!("ENGINE_REPLACE_TARGET: {label}: {error}"))?;
    }
    fs::rename(&part, target)
        .await
        .map_err(|error| format!("ENGINE_DOWNLOAD_FINALIZE: {label}: {error}"))
}

/// Unpack a downloaded archive while preserving a useful failure reason.
pub async fn extract_zip(archive: &Path, destination: &Path) -> Result<(), String> {
    let archive = archive.to_path_buf();
    let destination = destination.to_path_buf();
    tokio::task::spawn_blocking(move || extract_zip_blocking(&archive, &destination))
        .await
        .map_err(|error| format!("ENGINE_EXTRACT_TASK: {error}"))?
}

fn extract_zip_blocking(archive: &Path, destination: &Path) -> Result<(), String> {
    let failure = |reason: String| {
        format!(
            "ENGINE_EXTRACT_FAILED: extracting {} into {}: {reason}",
            archive.display(),
            destination.display()
        )
    };
    let file = std::fs::File::open(archive)
        .map_err(|error| failure(format!("could not open archive: {error}")))?;
    let mut zip =
        zip::ZipArchive::new(file).map_err(|error| failure(format!("invalid archive: {error}")))?;
    std::fs::create_dir_all(destination)
        .map_err(|error| failure(format!("could not create destination: {error}")))?;
    let mut archive_names = std::collections::HashSet::new();

    for index in 0..zip.len() {
        let mut entry = zip
            .by_index(index)
            .map_err(|error| failure(format!("could not read entry {index}: {error}")))?;
        let relative = entry
            .enclosed_name()
            .ok_or_else(|| failure(format!("unsafe archive path: {}", entry.name())))?
            .to_path_buf();
        let normalized = relative
            .components()
            .filter_map(|component| match component {
                Component::Normal(part) => Some(part.to_string_lossy().to_lowercase()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("/");
        if !archive_names.insert(normalized) {
            return Err(failure(format!("duplicate archive path: {}", entry.name())));
        }
        if entry
            .unix_mode()
            .is_some_and(|mode| mode & 0o170000 == 0o120000)
        {
            return Err(failure(format!(
                "symbolic links are not supported: {}",
                entry.name()
            )));
        }
        let output_path = destination.join(&relative);
        if entry.is_dir() {
            std::fs::create_dir_all(&output_path).map_err(|error| {
                failure(format!(
                    "could not create directory {}: {error}",
                    relative.display()
                ))
            })?;
            continue;
        }
        if let Some(parent) = output_path.parent() {
            std::fs::create_dir_all(parent).map_err(|error| {
                failure(format!(
                    "could not create parent for {}: {error}",
                    relative.display()
                ))
            })?;
        }
        let mut output = std::fs::File::create(&output_path).map_err(|error| {
            failure(format!("could not create {}: {error}", relative.display()))
        })?;
        std::io::copy(&mut entry, &mut output)
            .map_err(|error| failure(format!("could not write {}: {error}", relative.display())))?;
    }
    Ok(())
}

#[cfg(test)]
#[path = "engine_artifact_io_tests.rs"]
mod tests;
