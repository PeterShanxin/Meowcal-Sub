use crate::engine_manifest::SystemRequirements;
use std::path::Path;

#[derive(Debug, Clone, Copy)]
struct SystemSnapshot {
    windows_build: u32,
    total_ram_bytes: u64,
    free_disk_bytes: u64,
}

pub async fn run(
    install_root: &Path,
    requirements: &SystemRequirements,
    disk_space_needed: bool,
) -> Result<(), String> {
    if !install_root.is_absolute() {
        return Err("ENGINE_INCOMPATIBLE: install path must be absolute".to_string());
    }
    #[cfg(windows)]
    if install_root.to_string_lossy().starts_with(r"\\") {
        return Err("ENGINE_INCOMPATIBLE: network storage is not supported".to_string());
    }

    tokio::fs::create_dir_all(install_root)
        .await
        .map_err(|error| format!("ENGINE_PREFLIGHT_PATH: {error}"))?;
    let install_root = install_root.to_path_buf();
    let snapshot = tauri::async_runtime::spawn_blocking(move || snapshot(&install_root))
        .await
        .map_err(|error| format!("ENGINE_PREFLIGHT_TASK: {error}"))??;
    validate(snapshot, requirements, disk_space_needed)
}

fn validate(
    snapshot: SystemSnapshot,
    requirements: &SystemRequirements,
    disk_space_needed: bool,
) -> Result<(), String> {
    if snapshot.windows_build < requirements.minimum_windows_build {
        return Err(format!(
            "ENGINE_INCOMPATIBLE: Windows build {} or newer is required (found {})",
            requirements.minimum_windows_build, snapshot.windows_build
        ));
    }
    if snapshot.total_ram_bytes < requirements.minimum_ram_bytes {
        return Err(format!(
            "ENGINE_INCOMPATIBLE: at least {} GB RAM is required",
            requirements.minimum_ram_bytes / 1024 / 1024 / 1024
        ));
    }
    if disk_space_needed && snapshot.free_disk_bytes < requirements.minimum_free_disk_bytes {
        return Err(format!(
            "ENGINE_DISK_SPACE: at least {:.1} GB free is required",
            requirements.minimum_free_disk_bytes as f64 / 1024.0 / 1024.0 / 1024.0
        ));
    }
    Ok(())
}

#[cfg(windows)]
fn snapshot(path: &Path) -> Result<SystemSnapshot, String> {
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;
    use windows::core::PCWSTR;
    use windows::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;
    use windows::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};

    let wide_path: Vec<u16> = OsStr::new(path)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    let mut free_disk_bytes = 0u64;
    unsafe {
        GetDiskFreeSpaceExW(
            PCWSTR(wide_path.as_ptr()),
            Some(&mut free_disk_bytes),
            None,
            None,
        )
        .map_err(|error| format!("ENGINE_DISK_SPACE: {error}"))?;
    }

    let mut memory = MEMORYSTATUSEX {
        dwLength: std::mem::size_of::<MEMORYSTATUSEX>() as u32,
        ..MEMORYSTATUSEX::default()
    };
    unsafe {
        GlobalMemoryStatusEx(&mut memory)
            .map_err(|error| format!("ENGINE_INCOMPATIBLE: memory query failed: {error}"))?;
    }

    Ok(SystemSnapshot {
        windows_build: windows_version::OsVersion::current().build,
        total_ram_bytes: memory.ullTotalPhys,
        free_disk_bytes,
    })
}

#[cfg(not(windows))]
fn snapshot(_path: &Path) -> Result<SystemSnapshot, String> {
    Err("ENGINE_INCOMPATIBLE: Windows is required".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn requirements() -> SystemRequirements {
        SystemRequirements {
            minimum_windows_build: 22_000,
            minimum_ram_bytes: 8 * 1024 * 1024 * 1024,
            minimum_free_disk_bytes: 3 * 1024 * 1024 * 1024,
        }
    }

    #[test]
    fn rejects_unsupported_windows_ram_and_disk() {
        let requirements = requirements();
        let base = SystemSnapshot {
            windows_build: 22_000,
            total_ram_bytes: requirements.minimum_ram_bytes,
            free_disk_bytes: requirements.minimum_free_disk_bytes,
        };
        assert!(validate(
            SystemSnapshot {
                windows_build: 21_999,
                ..base
            },
            &requirements,
            true
        )
        .unwrap_err()
        .starts_with("ENGINE_INCOMPATIBLE"));
        assert!(validate(
            SystemSnapshot {
                total_ram_bytes: requirements.minimum_ram_bytes - 1,
                ..base
            },
            &requirements,
            true
        )
        .is_err());
        assert!(validate(
            SystemSnapshot {
                free_disk_bytes: requirements.minimum_free_disk_bytes - 1,
                ..base
            },
            &requirements,
            true
        )
        .unwrap_err()
        .starts_with("ENGINE_DISK_SPACE"));
    }

    #[test]
    fn adoption_does_not_require_install_headroom() {
        let requirements = requirements();
        assert!(validate(
            SystemSnapshot {
                windows_build: 22_000,
                total_ram_bytes: requirements.minimum_ram_bytes,
                free_disk_bytes: 0,
            },
            &requirements,
            false
        )
        .is_ok());
    }
}
