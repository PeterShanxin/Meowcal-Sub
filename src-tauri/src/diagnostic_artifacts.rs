//! Explicitly opted-in local OCR evidence, bounded independently from session logs.
use crate::logging_settings;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, OnceLock};
use tokio::sync::Semaphore;

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
    let success = offload(write_slots(), move || {
        if !logging_settings::verbose() {
            return Ok(());
        }
        let png = frame
            .map(|(bgra, width, height)| encode_frame(bgra, width, height))
            .transpose()?;
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
            logging_settings::verbose,
        )
    })
    .await;
    if !success {
        // Do not echo data or backend errors: they can contain captured text.
        tracing::warn!("Local verbose evidence could not be saved");
    }
}

fn write_slots() -> Arc<Semaphore> {
    static SLOTS: OnceLock<Arc<Semaphore>> = OnceLock::new();
    Arc::clone(SLOTS.get_or_init(|| Arc::new(Semaphore::new(2))))
}

async fn offload(
    slots: Arc<Semaphore>,
    write: impl FnOnce() -> io::Result<()> + Send + 'static,
) -> bool {
    let Ok(permit) = slots.acquire_owned().await else {
        return false;
    };
    matches!(
        tokio::task::spawn_blocking(move || {
            // A cancelled waiter must not release capacity while disk I/O continues.
            let _permit = permit;
            write()
        })
        .await,
        Ok(Ok(()))
    )
}

fn encode_frame(mut bgra: Vec<u8>, width: u32, height: u32) -> io::Result<Vec<u8>> {
    let expected = (u64::from(width) * u64::from(height)).checked_mul(4);
    if expected != Some(bgra.len() as u64)
        || bgra.len() > MAX_FRAME_BYTES
        || width == 0
        || height == 0
    {
        return Err(io::Error::other("Invalid diagnostics frame"));
    }
    for pixel in bgra.chunks_exact_mut(4) {
        pixel.swap(0, 2);
        // GDI's unused alpha byte can be zero; screen evidence is opaque.
        pixel[3] = 255;
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
    Ok(bytes)
}

fn is_link(metadata: &std::fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        metadata.file_attributes() & 0x400 != 0
    }
    #[cfg(not(windows))]
    {
        metadata.file_type().is_symlink()
    }
}

fn plain_directory(path: &Path) -> io::Result<()> {
    match std::fs::symlink_metadata(path) {
        Ok(meta) if meta.is_dir() && !is_link(&meta) => Ok(()),
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
        let meta = std::fs::symlink_metadata(entry.path())?;
        if !meta.is_dir() || is_link(&meta) {
            continue;
        }
        let mut size = 0;
        for file in std::fs::read_dir(entry.path())? {
            let file = file?;
            let kind = std::fs::symlink_metadata(file.path())?;
            if !kind.is_file()
                || is_link(&kind)
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

fn lock_writer(file: &std::fs::File, wait: std::time::Duration) -> io::Result<()> {
    let start = std::time::Instant::now();
    loop {
        match file.try_lock() {
            Ok(()) => return Ok(()),
            Err(std::fs::TryLockError::WouldBlock) if start.elapsed() < wait => {
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            Err(error) => return Err(io::Error::other(error)),
        }
    }
}

fn write_record(
    root: &Path,
    json: &[u8],
    png: Option<&[u8]>,
    limit: u64,
    allowed: impl FnOnce() -> bool,
) -> io::Result<()> {
    let incoming = json.len() as u64 + png.map_or(0, |bytes| bytes.len() as u64);
    if incoming > limit {
        return Err(io::Error::other("Diagnostics record exceeds storage limit"));
    }
    plain_directory(root)?;
    let lock_path = root.join(".writer.lock");
    if std::fs::symlink_metadata(&lock_path)
        .is_ok_and(|meta| !meta.is_file() || is_link(&meta))
    {
        return Err(io::Error::other("Invalid diagnostics lock"));
    }
    let lock = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(lock_path)?;
    lock_writer(&lock, std::time::Duration::from_secs(2))?;
    if !allowed() {
        return Ok(());
    }
    let previous = records(root)?;
    let mut total: u64 = previous.iter().map(|(_, size)| size).sum();
    for (directory, size) in previous {
        if total.saturating_add(incoming) <= limit {
            break;
        }
        plain_directory(&directory)?;
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
