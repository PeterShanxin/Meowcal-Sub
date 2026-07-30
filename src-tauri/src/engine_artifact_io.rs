use crate::engine_manifest::{DownloadArtifact, InstalledExecutable};
use reqwest::Client;
use sha2::{Digest, Sha256};
use std::io::Read;
use std::path::Path;
use std::time::Duration;
use tauri::{AppHandle, Emitter, Runtime};
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
    let digest = tauri::async_runtime::spawn_blocking(move || sha256_file(&owned_path))
        .await
        .map_err(|error| format!("ENGINE_VERIFY_TASK: {error}"))??;
    Ok(digest.eq_ignore_ascii_case(expected_hash))
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

pub async fn download_file<R: Runtime>(
    app: &AppHandle<R>,
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
        .user_agent("Meowcal-Sub/0.5.0")
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
                emit_progress(app, format!("{label}: {}%", percent.min(100)));
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

pub async fn extract_zip(archive: &Path, destination: &Path) -> Result<(), String> {
    let script = "param([string]$archive,[string]$destination) \
                  Expand-Archive -LiteralPath $archive -DestinationPath $destination -Force";
    let status = tokio::process::Command::new("powershell")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            script,
            archive.to_string_lossy().as_ref(),
            destination.to_string_lossy().as_ref(),
        ])
        .status()
        .await
        .map_err(|error| format!("ENGINE_EXTRACT_START: {error}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("ENGINE_EXTRACT_FAILED: {:?}", status.code()))
    }
}

fn emit_progress<R: Runtime>(app: &AppHandle<R>, line: impl Into<String>) {
    let _ = app.emit_to(
        "foundry-wizard",
        "wizard-output",
        serde_json::json!({"stream": "stdout", "line": line.into()}),
    );
}

fn sha256_file(path: &Path) -> Result<String, String> {
    let mut file =
        std::fs::File::open(path).map_err(|error| format!("ENGINE_VERIFY_OPEN: {error}"))?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 1024 * 1024];
    loop {
        let count = file
            .read(&mut buffer)
            .map_err(|error| format!("ENGINE_VERIFY_READ: {error}"))?;
        if count == 0 {
            break;
        }
        hasher.update(&buffer[..count]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine_manifest::EngineManifest;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[tokio::test]
    async fn same_sized_corrupt_artifact_is_not_treated_as_installed() {
        let path = fixture_path("integrity", "bin");
        std::fs::write(&path, b"trusted").unwrap();
        let expected_hash = sha256_file(&path).unwrap();
        assert!(file_matches(&path, 7, &expected_hash).await.unwrap());
        std::fs::write(&path, b"corrupt").unwrap();
        assert!(!file_matches(&path, 7, &expected_hash).await.unwrap());
        std::fs::remove_file(path).unwrap();
    }

    #[tokio::test]
    async fn corrupt_runtime_executable_requires_repair() {
        let manifest = EngineManifest::shipped().unwrap();
        let runtime = manifest.runtime_for_current_arch().unwrap();
        let path = fixture_path("runtime-corrupt", "exe");
        std::fs::write(&path, vec![0; runtime.executable.size_bytes as usize]).unwrap();
        assert!(!file_matches(
            &path,
            runtime.executable.size_bytes,
            &runtime.executable.sha256
        )
        .await
        .unwrap());
        std::fs::remove_file(path).unwrap();
    }

    fn fixture_path(label: &str, extension: &str) -> std::path::PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "meowcal-{label}-{}-{unique}.{extension}",
            std::process::id()
        ))
    }
}
