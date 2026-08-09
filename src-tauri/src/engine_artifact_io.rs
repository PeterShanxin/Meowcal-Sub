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
    //
    // The paths arrive through the environment rather than as arguments after
    // `-Command`. `-Command` does not bind trailing arguments to a `param()`
    // block - it appends them to the command text - so the previous form ran
    // `Expand-Archive -LiteralPath '' -DestinationPath ''` and failed every time
    // with "the argument is null or empty". Passing them as variables also means
    // a path containing a quote or a space cannot alter the command.
    let script = "$ErrorActionPreference = 'Stop'; \
                  try { \
                    Expand-Archive -LiteralPath $env:MEOWCAL_EXTRACT_ARCHIVE \
                      -DestinationPath $env:MEOWCAL_EXTRACT_DESTINATION -Force \
                  } catch { Write-Error $_; exit 1 }";
    // Captured rather than inherited: `.status()` let PowerShell's diagnosis go
    // to a console nobody was reading.
    let output = crate::windowless_command::tokio_command("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", script])
        .env("MEOWCAL_EXTRACT_ARCHIVE", archive)
        .env("MEOWCAL_EXTRACT_DESTINATION", destination)
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
        failure_reason(&output.stderr, &output.stdout)
    ))
}

/// The reason a failed extraction reported, wherever it landed.
///
/// PowerShell writes cmdlet errors to stderr, but a native tool invoked beneath
/// it can report on stdout and exit non-zero. Falling back keeps the message
/// useful instead of reporting "no error output" next to a real failure.
fn failure_reason(stderr: &[u8], stdout: &[u8]) -> String {
    let reason = extraction_reason(stderr);
    if reason == NO_OUTPUT {
        let fallback = extraction_reason(stdout);
        if fallback != NO_OUTPUT {
            return fallback;
        }
    }
    reason
}

const NO_OUTPUT: &str = "no error output";

/// A line of PowerShell's error trailer rather than the error itself.
///
/// Windows PowerShell follows every error record with a fixed block: the source
/// line, a `~~~~` marker under the offending token, `CategoryInfo`, and
/// `FullyQualifiedErrorId`. None of it says what went wrong, and all of it sits
/// *after* the sentence that does.
fn is_trace_noise(line: &str) -> bool {
    line.starts_with('+') || (line.starts_with("At ") && line.contains("char:"))
}

/// The useful part of a PowerShell failure.
///
/// Issue #66 again: keeping "the last few lines" looked right against a short
/// synthetic fixture and was wrong against a real one, because the trailer above
/// is exactly four lines long. A genuine failure therefore arrived as
/// `CategoryInfo ...; FullyQualifiedErrorId ...`, which is the same as reporting
/// nothing. The trailer is dropped first, and the tail of what remains is kept -
/// so a cause at the top of a PowerShell record and a cause at the end of some
/// other tool's output both survive.
///
/// Bounded by lines and by characters, because the wizard shows this text and a
/// single enormous line would bury the dialog just as effectively as a trace.
fn extraction_reason(stderr: &[u8]) -> String {
    const MAX_LINES: usize = 4;
    const MAX_CHARS: usize = 600;

    let text = String::from_utf8_lossy(stderr);
    let lines: Vec<&str> = text
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect();
    if lines.is_empty() {
        return NO_OUTPUT.to_string();
    }

    // If every line is trailer, something is better than nothing: report what
    // there is rather than claiming there was no output at all.
    let meaningful: Vec<&str> = lines
        .iter()
        .copied()
        .filter(|line| !is_trace_noise(line))
        .collect();
    let kept = if meaningful.is_empty() {
        lines
    } else {
        meaningful
    };

    let joined = kept[kept.len().saturating_sub(MAX_LINES)..].join("; ");

    // Windows PowerShell writes each record as `<source> : <message>`, and for a
    // `-Command` invocation the source is the whole script echoed back. That is
    // a paragraph of our own code in front of the one sentence the operator
    // needs, so the message is taken from after the first separator. Output that
    // carries no separator is left exactly as it is.
    let reason = match joined.split_once(" : ") {
        Some((_, message)) if !message.trim().is_empty() => message.trim().to_string(),
        _ => joined,
    };

    // Keeping the first MAX_CHARS would have thrown away the cause whenever
    // PowerShell put the whole diagnosis on one long line - and it does, because
    // Expand-Archive quotes both paths before saying what went wrong. Both ends
    // are kept: the start names what failed, the end says why.
    let characters: Vec<char> = reason.chars().collect();
    if characters.len() <= MAX_CHARS {
        return reason;
    }
    let head: String = characters[..MAX_CHARS / 2].iter().collect();
    let tail: String = characters[characters.len() - (MAX_CHARS - MAX_CHARS / 2)..]
        .iter()
        .collect();
    format!("{head} ... {tail}")
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
#[path = "engine_artifact_io_tests.rs"]
mod tests;
