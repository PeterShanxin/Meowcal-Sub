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

/// `file_matches` without a runtime to await on.
///
/// Startup recovery (`engine_recovery`) has to decide whether an install is
/// trustworthy before Tauri's `setup` returns, and it decides whether to launch
/// an executable - so a size check alone is not enough. Hashing the model costs
/// seconds, which is why this is not on the normal startup path: it runs only
/// when a registration has been lost and an install is about to be re-adopted.
pub fn file_matches_blocking(path: &Path, expected_size: u64, expected_hash: &str) -> bool {
    if path
        .metadata()
        .map(|metadata| metadata.len() != expected_size)
        .unwrap_or(true)
    {
        return false;
    }
    sha256_file(path)
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

/// Unpack a downloaded archive.
///
/// Issue #66: this used to report `ENGINE_EXTRACT_FAILED: Some(1)` and nothing
/// else. `Expand-Archive` distinguishes a locked destination from a full disk
/// from a truncated archive, and every one of them arrived as the same exit
/// code, naming neither the archive nor where it was being written. Setup is the
/// one place a viewer cannot work around a failure themselves, so the reason has
/// to survive.
pub async fn extract_zip(archive: &Path, destination: &Path) -> Result<(), String> {
    // `powershell -Command` exits 0 for a non-terminating cmdlet error, which is
    // what most `Expand-Archive` failures are - a locked destination writes to
    // stderr, extracts nothing, and reports success. The install then failed one
    // step later as "executable missing after extraction", throwing away the
    // stderr that says why. Promoting errors to terminating and exiting non-zero
    // is what makes the captured output below actually reachable.
    let script = "param([string]$archive,[string]$destination) \
                  $ErrorActionPreference = 'Stop'; \
                  try { Expand-Archive -LiteralPath $archive -DestinationPath $destination -Force } \
                  catch { Write-Error $_; exit 1 }";
    // Captured rather than inherited: `.status()` let PowerShell's diagnosis go
    // to a console nobody was reading.
    let output = crate::windowless_command::tokio_command("powershell")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            script,
            archive.to_string_lossy().as_ref(),
            destination.to_string_lossy().as_ref(),
        ])
        .output()
        .await
        .map_err(|error| format!("ENGINE_EXTRACT_START: {error}"))?;
    if output.status.success() {
        return Ok(());
    }
    Err(format!(
        "ENGINE_EXTRACT_FAILED: {:?} extracting {} into {}: {}",
        output.status.code(),
        archive.display(),
        destination.display(),
        extraction_reason(&output.stderr)
    ))
}

/// The useful part of a PowerShell failure.
///
/// `Expand-Archive` prefixes each error with a banner and then wraps the detail
/// across several lines; the last non-empty lines carry the cause. Capped so a
/// stack trace cannot bury the message in a wizard dialog.
fn extraction_reason(stderr: &[u8]) -> String {
    const MAX_LINES: usize = 4;
    let text = String::from_utf8_lossy(stderr);
    let lines: Vec<&str> = text
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect();
    if lines.is_empty() {
        return "no error output".to_string();
    }
    lines[lines.len().saturating_sub(MAX_LINES)..].join("; ")
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

    // Issue #66: the reason has to reach the wizard, not a console nobody sees.
    #[test]
    fn an_extraction_failure_keeps_the_reason() {
        let stderr = b"Expand-Archive : The process cannot access the file \
                       'llama.dll' because it is being used by another process.\n\
                       At line:1 char:1\n";
        let reason = extraction_reason(stderr);

        assert!(reason.contains("being used by another process"));
    }

    // A silent failure must still say that it was silent, rather than trailing
    // off into an empty string that reads like a truncated message.
    #[test]
    fn an_extraction_failure_without_output_says_so() {
        assert_eq!(extraction_reason(b""), "no error output");
    }

    // PowerShell can emit a long trace; the wizard shows this text, so the tail
    // that carries the cause is kept and the rest dropped.
    #[test]
    fn a_long_trace_is_trimmed_to_its_last_lines() {
        let stderr = (1..=20)
            .map(|n| format!("line {n}"))
            .collect::<Vec<_>>()
            .join("\n");
        let reason = extraction_reason(stderr.as_bytes());

        assert_eq!(reason, "line 17; line 18; line 19; line 20");
    }

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
