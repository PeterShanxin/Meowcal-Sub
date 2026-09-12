use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use tauri::{Manager, Runtime};

pub fn register<R: Runtime>(app: &tauri::AppHandle<R>) -> Result<(), String> {
    let profile = if app.config().identifier.ends_with(".dev") {
        "development"
    } else {
        "production"
    };
    let resource_dir = app
        .path()
        .resource_dir()
        .map_err(|error| format!("CORE_RESOURCE_DIR: {error}"))?;
    set_launch(resolve_executable(profile, &resource_dir)?, profile)
}

pub fn register_headless(
    storage_root: Option<PathBuf>,
    legacy_roots: Vec<PathBuf>,
) -> Result<(), String> {
    set_launch(
        resolve_executable("development", Path::new(""))?,
        "development",
    )?;
    configure_storage(storage_root, legacy_roots)
}

fn set_launch(executable: PathBuf, profile: &'static str) -> Result<(), String> {
    if !executable.is_file() {
        return Err(format!("CORE_EXECUTABLE_MISSING: {}", executable.display()));
    }
    if !super::CORE_SOURCE_CANDIDATE {
        validate_reviewed_resource(
            &executable,
            env!("MEOWCAL_EXPECTED_CORE_EXECUTABLE_SHA256"),
            env!("MEOWCAL_EXPECTED_CORE_LICENSE_SHA256"),
        )?;
    }
    let mut config = super::CONFIG
        .get_or_init(|| Mutex::new(None))
        .lock()
        .map_err(|_| "CORE_CONFIG_LOCK_POISONED".to_string())?;
    *config = Some(super::LaunchConfig {
        executable,
        profile,
        storage_root: None,
        legacy_roots: Vec::new(),
    });
    Ok(())
}

pub fn configure_storage(
    storage_root: Option<PathBuf>,
    legacy_roots: Vec<PathBuf>,
) -> Result<(), String> {
    validate_paths(storage_root.as_ref(), &legacy_roots)?;
    if process_started() {
        return Err("CORE_ALREADY_STARTED: storage cannot change after hello".to_string());
    }
    let mut guard = super::CONFIG
        .get_or_init(|| Mutex::new(None))
        .lock()
        .map_err(|_| "CORE_CONFIG_LOCK_POISONED".to_string())?;
    let config = guard
        .as_mut()
        .ok_or_else(|| "CORE_NOT_REGISTERED".to_string())?;
    config.storage_root = storage_root;
    config.legacy_roots = dedupe_paths(legacy_roots);
    Ok(())
}

fn process_started() -> bool {
    [&super::TRANSLATION, &super::OCR].iter().any(|slot| {
        slot.get()
            .and_then(|slot| slot.lock().ok())
            .is_some_and(|slot| slot.is_some())
    })
}

pub fn select_storage_root(storage_root: Option<PathBuf>) -> Result<(), String> {
    validate_paths(storage_root.as_ref(), &[])?;
    let unchanged = super::CONFIG
        .get()
        .and_then(|config| config.lock().ok())
        .and_then(|config| {
            config
                .as_ref()
                .map(|config| config.storage_root == storage_root)
        })
        .unwrap_or(false);
    if unchanged {
        return Ok(());
    }
    super::shutdown_owned();
    let mut guard = super::CONFIG
        .get_or_init(|| Mutex::new(None))
        .lock()
        .map_err(|_| "CORE_CONFIG_LOCK_POISONED".to_string())?;
    let config = guard
        .as_mut()
        .ok_or_else(|| "CORE_NOT_REGISTERED".to_string())?;
    config.storage_root = storage_root;
    Ok(())
}

pub(super) fn validate_paths(storage: Option<&PathBuf>, legacy: &[PathBuf]) -> Result<(), String> {
    if storage.is_some_and(|path| !path.is_absolute())
        || legacy.iter().any(|path| !path.is_absolute())
    {
        return Err("CORE_STORAGE_PATH: every storage path must be absolute".to_string());
    }
    Ok(())
}

pub(super) fn dedupe_paths(paths: Vec<PathBuf>) -> Vec<PathBuf> {
    paths.into_iter().fold(Vec::new(), |mut unique, path| {
        if !unique.contains(&path) {
            unique.push(path);
        }
        unique
    })
}

pub(super) fn resolve_executable(profile: &str, resource_dir: &Path) -> Result<PathBuf, String> {
    if let Some(path) = std::env::var_os("MEOWCAL_CORE_EXECUTABLE") {
        if profile != "development" && !cfg!(test) {
            return Err(
                "CORE_OVERRIDE_FORBIDDEN: MEOWCAL_CORE_EXECUTABLE is development-only".to_string(),
            );
        }
        return Ok(PathBuf::from(path));
    }
    if profile == "production" || !super::CORE_SOURCE_CANDIDATE {
        return Ok(resource_dir.join("resources/core/meowcal-core.exe"));
    }
    let target = if cfg!(target_arch = "aarch64") {
        "aarch64-pc-windows-msvc"
    } else {
        "x86_64-pc-windows-msvc"
    };
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .ok_or_else(|| "CORE_DEV_PATH: repository root unavailable".to_string())?;
    Ok(root
        .join("core/target")
        .join(target)
        .join("release/meowcal-core.exe"))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ReviewedCoreMetadata {
    schema_version: u32,
    core_version: String,
    api_version: u32,
    os: String,
    architecture: String,
    executable: String,
    executable_sha256: String,
    license: String,
    license_sha256: String,
}

pub(super) fn validate_reviewed_resource(
    executable: &Path,
    expected_executable_hash: &str,
    expected_license_hash: &str,
) -> Result<(), String> {
    let resource_directory = executable.parent().ok_or_else(|| {
        "CORE_RELEASE_METADATA_MISSING: executable has no parent directory".to_string()
    })?;
    let metadata_path = resource_directory.join("meowcal-core.json");
    let metadata = std::fs::read_to_string(&metadata_path)
        .map_err(|error| format!("CORE_RELEASE_METADATA_MISSING: {error}"))?;
    let metadata: ReviewedCoreMetadata = serde_json::from_str(&metadata)
        .map_err(|error| format!("CORE_RELEASE_METADATA_INVALID: {error}"))?;
    let architecture = if cfg!(target_arch = "aarch64") {
        "arm64"
    } else {
        "x64"
    };
    if metadata.schema_version != 1
        || metadata.core_version != super::CORE_VERSION
        || metadata.api_version != super::API_VERSION
        || metadata.os != "windows"
        || metadata.architecture != architecture
        || metadata.executable != "meowcal-core.exe"
        || metadata.license != "LICENSE"
        || !is_sha256(&metadata.executable_sha256)
        || !is_sha256(&metadata.license_sha256)
        || metadata.executable_sha256 != expected_executable_hash
        || metadata.license_sha256 != expected_license_hash
    {
        return Err(
            "CORE_RELEASE_METADATA_INVALID: reviewed Core resource does not match the compiled pin"
                .to_string(),
        );
    }
    if sha256_file(executable)? != metadata.executable_sha256
        || sha256_file(&resource_directory.join(&metadata.license))? != metadata.license_sha256
    {
        return Err(
            "CORE_RELEASE_RESOURCE_MISMATCH: reviewed Core resource digest does not match metadata"
                .to_string(),
        );
    }
    Ok(())
}

pub(super) fn sha256_file(path: &Path) -> Result<String, String> {
    let mut file = File::open(path)
        .map_err(|error| format!("CORE_RELEASE_RESOURCE_MISSING: {}: {error}", path.display()))?;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 8192];
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|error| format!("CORE_RELEASE_RESOURCE_READ: {}: {error}", path.display()))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn is_sha256(value: &str) -> bool {
    value.len() == 64
        && value != "0".repeat(64)
        && value.bytes().all(|byte| {
            byte.is_ascii_digit() || (byte.is_ascii_lowercase() && byte.is_ascii_hexdigit())
        })
}
