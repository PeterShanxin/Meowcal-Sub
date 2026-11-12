//! Explicitly opted-in local OCR evidence, bounded independently from session logs.
use crate::logging_settings;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

const MAX_BYTES: u64 = 256 * 1024 * 1024;
const MAX_FRAME_BYTES: usize = 64 * 1024 * 1024;
static NEXT: AtomicU64 = AtomicU64::new(0);
tokio::task_local! { static FRAME: (u64, u64); }

pub async fn capture<F: std::future::Future>(
    session_id: u64,
    capture_id: u64,
    frame: &crate::capture::CaptureResult,
    operation: F,
) -> F::Output {
    FRAME
        .scope((session_id, capture_id), async {
            if logging_settings::verbose() {
                record(
                    "capture",
                    serde_json::json!({}),
                    Some((frame.data.clone(), frame.width, frame.height)),
                )
                .await;
            }
            operation.await
        })
        .await
}

pub async fn record(
    kind: &'static str,
    detail: serde_json::Value,
    frame: Option<(Vec<u8>, u32, u32)>,
) {
    if !logging_settings::verbose() {
        return;
    }
    let identity = FRAME.try_with(|identity| *identity).ok();
    let task = tokio::task::spawn_blocking(move || {
        if !logging_settings::verbose() {
            return Ok(());
        }
        let png = match frame {
            Some((mut bgra, width, height)) => {
                let expected = u64::from(width) * u64::from(height) * 4;
                if expected != bgra.len() as u64
                    || bgra.len() > MAX_FRAME_BYTES
                    || width == 0
                    || height == 0
                {
                    return Err(io::Error::other("Invalid diagnostics frame"));
                }
                for pixel in bgra.chunks_exact_mut(4) {
                    pixel.swap(0, 2);
                }
                let mut bytes = Vec::new();
                image::ImageEncoder::write_image(
                    image::codecs::png::PngEncoder::new(&mut bytes),
                    &bgra,
                    width,
                    height,
                    image::ExtendedColorType::Rgba8,
                )
                .map_err(io::Error::other)?;
                Some(bytes)
            }
            None => None,
        };
        let json = serde_json::to_vec_pretty(&serde_json::json!({
            "kind": kind, "profile": crate::app_logging::profile_label(),
            "timestamp": chrono::Utc::now().to_rfc3339(), "frame": identity, "detail": detail,
        }))
        .map_err(io::Error::other)?;
        if !logging_settings::verbose() {
            return Ok(());
        }
        write_record(
            &crate::app_logging::resolve_log_dir().join("verbose"),
            &json,
            png.as_deref(),
            MAX_BYTES,
        )
    })
    .await;
    if !matches!(task, Ok(Ok(()))) {
        // Do not echo data or backend errors: they can contain captured text.
        tracing::warn!("Local verbose evidence could not be saved");
    }
}

fn plain_directory(path: &Path) -> io::Result<()> {
    match std::fs::symlink_metadata(path) {
        Ok(meta) if meta.is_dir() && !meta.file_type().is_symlink() => Ok(()),
        Ok(_) => Err(io::Error::other(
            "Diagnostics directory is not a plain directory",
        )),
        Err(error) if error.kind() == io::ErrorKind::NotFound => std::fs::create_dir_all(path),
        Err(error) => Err(error),
    }
}

fn owned_name(name: &str) -> bool {
    name.strip_prefix("record-").is_some_and(|suffix| {
        let parts: Vec<_> = suffix.split('-').collect();
        parts.len() == 3
            && parts
                .iter()
                .all(|part| !part.is_empty() && part.bytes().all(|c| c.is_ascii_digit()))
    })
}

fn records(root: &Path) -> io::Result<Vec<(PathBuf, u64)>> {
    let mut records = Vec::new();
    for entry in std::fs::read_dir(root)? {
        let entry = entry?;
        if !owned_name(&entry.file_name().to_string_lossy()) {
            continue;
        }
        let meta = entry.file_type()?;
        if !meta.is_dir() || meta.is_symlink() {
            continue;
        }
        let mut size = 0;
        for file in std::fs::read_dir(entry.path())? {
            let file = file?;
            let kind = file.file_type()?;
            if !kind.is_file()
                || kind.is_symlink()
                || !matches!(file.file_name().to_str(), Some("record.json" | "input.png"))
            {
                return Err(io::Error::other(
                    "Unexpected file in diagnostics record; preserved",
                ));
            }
            size += file.metadata()?.len();
        }
        records.push((entry.path(), size));
    }
    records.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(records)
}

fn write_record(root: &Path, json: &[u8], png: Option<&[u8]>, limit: u64) -> io::Result<()> {
    let incoming = json.len() as u64 + png.map_or(0, |bytes| bytes.len() as u64);
    if incoming > limit {
        return Err(io::Error::other("Diagnostics record exceeds storage limit"));
    }
    plain_directory(root)?;
    let lock_path = root.join(".writer.lock");
    if std::fs::symlink_metadata(&lock_path)
        .is_ok_and(|meta| !meta.is_file() || meta.file_type().is_symlink())
    {
        return Err(io::Error::other("Invalid diagnostics lock"));
    }
    let lock = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(lock_path)?;
    lock.lock()?;
    let previous = records(root)?;
    let mut total: u64 = previous.iter().map(|(_, size)| size).sum();
    for (directory, size) in previous {
        if total.saturating_add(incoming) <= limit {
            break;
        }
        for name in ["record.json", "input.png"] {
            match std::fs::remove_file(directory.join(name)) {
                Ok(()) => (),
                Err(error) if error.kind() == io::ErrorKind::NotFound => (),
                Err(error) => return Err(error),
            }
        }
        std::fs::remove_dir(directory)?;
        total -= size;
    }
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(io::Error::other)?
        .as_nanos();
    let directory = root.join(format!(
        "record-{timestamp:020}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir(&directory)?;
    let write = || -> io::Result<()> {
        let mut file = std::fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(directory.join("record.json"))?;
        file.write_all(json)?;
        if let Some(bytes) = png {
            let mut file = std::fs::OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(directory.join("input.png"))?;
            file.write_all(bytes)?;
        }
        Ok(())
    };
    write()
}

#[cfg(test)]
#[path = "diagnostic_artifacts_tests.rs"]
mod tests;
