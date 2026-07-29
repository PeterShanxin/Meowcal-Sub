use crate::engine_manifest::{DownloadArtifact, EngineManifest, InstalledExecutable};
use crate::hy_mt_runtime::HyMtInstallPaths;
use reqwest::Client;
use sha2::{Digest, Sha256};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager};
use tokio::fs;
use tokio::io::AsyncWriteExt;

pub async fn install(
    app: &AppHandle,
    cache_dir: Option<String>,
) -> Result<HyMtInstallPaths, String> {
    let manifest = EngineManifest::shipped().map_err(|error| error.to_string())?;
    let runtime = manifest
        .runtime_for_current_arch()
        .map_err(|error| error.to_string())?;
    let cache_root = resolve_cache_root(app, cache_dir)?;
    let paths = HyMtInstallPaths::from_cache_root(cache_root, &manifest, runtime);
    fs::create_dir_all(&paths.runtime_dir)
        .await
        .map_err(|error| format!("ENGINE_CREATE_RUNTIME_DIR: {error}"))?;
    fs::create_dir_all(&paths.model_dir)
        .await
        .map_err(|error| format!("ENGINE_CREATE_MODEL_DIR: {error}"))?;
    if let Some(parent) = paths.runtime_archive.parent() {
        fs::create_dir_all(parent)
            .await
            .map_err(|error| format!("ENGINE_CREATE_DOWNLOAD_DIR: {error}"))?;
    }

    let runtime_verified = file_matches(
        &paths.runtime_archive,
        runtime.archive.size_bytes,
        &runtime.archive.sha256,
    )
    .await?;
    if !runtime_verified {
        emit_progress(app, "Downloading the translation runtime...");
        download_file(
            app,
            &runtime.archive.url,
            &paths.runtime_archive,
            Some(runtime.archive.size_bytes),
            "Runtime",
        )
        .await?;
        verify_download(&paths.runtime_archive, &runtime.archive, "RUNTIME").await?;
    }
    let executable_verified = file_matches(
        &paths.executable,
        runtime.executable.size_bytes,
        &runtime.executable.sha256,
    )
    .await?;
    if executable_verified {
        emit_progress(app, "Translation runtime verified.");
    } else {
        emit_progress(app, "Installing or repairing the translation runtime...");
        extract_zip(&paths.runtime_archive, &paths.runtime_dir).await?;
        if !paths.executable.is_file() {
            return Err("ENGINE_RUNTIME_INVALID: executable missing after extraction".to_string());
        }
    }
    verify_executable(&paths.executable, &runtime.executable).await?;

    let model_verified = file_matches(
        &paths.model,
        manifest.model.artifact.size_bytes,
        &manifest.model.artifact.sha256,
    )
    .await?;
    if !model_verified {
        emit_progress(app, "Downloading Tencent HY-MT (about 1.1 GB)...");
        download_file(
            app,
            &manifest.model.artifact.url,
            &paths.model,
            Some(manifest.model.artifact.size_bytes),
            "Model",
        )
        .await?;
    } else {
        emit_progress(app, "HY-MT model already downloaded.");
    }

    emit_progress(app, "Verifying HY-MT...");
    verify_download(&paths.model, &manifest.model.artifact, "MODEL").await?;
    emit_progress(app, "Local Translation Engine verified.");
    Ok(paths)
}

async fn file_matches(
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

async fn verify_download(
    path: &Path,
    artifact: &DownloadArtifact,
    label: &str,
) -> Result<(), String> {
    verify_file(path, artifact.size_bytes, &artifact.sha256, label).await
}

async fn verify_executable(path: &Path, executable: &InstalledExecutable) -> Result<(), String> {
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

fn resolve_cache_root(app: &AppHandle, cache_dir: Option<String>) -> Result<PathBuf, String> {
    let root = match cache_dir.as_deref().map(str::trim) {
        Some(path) if !path.is_empty() && !path.contains('%') => PathBuf::from(path),
        _ => app
            .path()
            .app_cache_dir()
            .map_err(|error| format!("ENGINE_CACHE_PATH: {error}"))?,
    };
    if !root.is_absolute() {
        return Err("ENGINE_CACHE_PATH: storage directory must be absolute".to_string());
    }
    Ok(root)
}

fn emit_progress(app: &AppHandle, line: impl Into<String>) {
    let _ = app.emit_to(
        "foundry-wizard",
        "wizard-output",
        serde_json::json!({"stream": "stdout", "line": line.into()}),
    );
}

async fn download_file(
    app: &AppHandle,
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

async fn extract_zip(archive: &Path, destination: &Path) -> Result<(), String> {
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
    use std::time::{SystemTime, UNIX_EPOCH};

    #[tokio::test]
    async fn same_sized_corrupt_artifact_is_not_treated_as_installed() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock should be after epoch")
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "meowcal-engine-integrity-{}-{unique}.bin",
            std::process::id()
        ));
        std::fs::write(&path, b"trusted").expect("fixture should be writable");
        let expected_hash = sha256_file(&path).expect("fixture should be hashable");

        assert!(file_matches(&path, 7, &expected_hash)
            .await
            .expect("matching fixture should verify"));

        std::fs::write(&path, b"corrupt").expect("fixture should be replaceable");
        assert!(!file_matches(&path, 7, &expected_hash)
            .await
            .expect("corrupt fixture should be classified"));

        std::fs::remove_file(path).expect("fixture should be removable");
    }

    #[tokio::test]
    async fn corrupt_runtime_executable_requires_repair() {
        let manifest = EngineManifest::shipped().expect("manifest should validate");
        let runtime = manifest
            .runtime_for_current_arch()
            .expect("current architecture should be supported");
        let path = std::env::temp_dir().join(format!(
            "meowcal-runtime-corrupt-{}.exe",
            std::process::id()
        ));
        std::fs::write(&path, vec![0; runtime.executable.size_bytes as usize])
            .expect("fixture should be writable");

        assert!(!file_matches(
            &path,
            runtime.executable.size_bytes,
            &runtime.executable.sha256
        )
        .await
        .expect("corrupt executable should be classified"));

        std::fs::remove_file(path).expect("fixture should be removable");
    }
}
