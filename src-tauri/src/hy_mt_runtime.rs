use crate::config::ManagedLocalRuntimeConfig;
use crate::engine_manifest::{EngineManifest, RuntimeSpec};
use reqwest::Client;
use std::fs::OpenOptions;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{Mutex, OnceLock};
use std::time::Duration;
use tokio::time::{sleep, Instant};
use tracing::{info, warn};

static OWNED_RUNTIME: OnceLock<Mutex<Option<Child>>> = OnceLock::new();

#[derive(Debug, Clone)]
pub struct HyMtInstallPaths {
    pub root: PathBuf,
    pub runtime_dir: PathBuf,
    pub runtime_archive: PathBuf,
    pub executable: PathBuf,
    pub model_dir: PathBuf,
    pub model: PathBuf,
}

impl HyMtInstallPaths {
    pub fn from_cache_root(
        cache_root: impl AsRef<Path>,
        manifest: &EngineManifest,
        runtime: &RuntimeSpec,
    ) -> Self {
        let root = cache_root.as_ref().join("meowcal-sub");
        let runtime_dir = root.join("runtime").join(&runtime.install_directory);
        let model_dir = root.join("models").join(&manifest.model.install_directory);
        Self {
            runtime_archive: root.join("runtime").join(&runtime.archive.file_name),
            executable: runtime_dir.join(&runtime.executable.relative_path),
            model: model_dir.join(&manifest.model.artifact.file_name),
            root,
            runtime_dir,
            model_dir,
        }
    }

    pub fn is_complete(&self, manifest: &EngineManifest, runtime: &RuntimeSpec) -> bool {
        self.executable
            .metadata()
            .map(|metadata| metadata.len() == runtime.executable.size_bytes)
            .unwrap_or(false)
            && self
                .model
                .metadata()
                .map(|metadata| metadata.len() == manifest.model.artifact.size_bytes)
                .unwrap_or(false)
    }

    pub fn managed_config(&self, manifest: &EngineManifest) -> ManagedLocalRuntimeConfig {
        ManagedLocalRuntimeConfig {
            kind: "hy-mt".to_string(),
            executable_path: self.executable.to_string_lossy().to_string(),
            model_path: self.model.to_string_lossy().to_string(),
            port: manifest.launch.preferred_port,
        }
    }
}

pub fn endpoint_url(runtime: &ManagedLocalRuntimeConfig) -> String {
    format!("http://127.0.0.1:{}", runtime.port)
}

pub async fn is_healthy(runtime: &ManagedLocalRuntimeConfig) -> bool {
    let client = Client::builder()
        .timeout(Duration::from_secs(2))
        .build()
        .unwrap_or_default();
    client
        .get(format!("{}/health", endpoint_url(runtime)))
        .send()
        .await
        .map(|response| response.status().is_success())
        .unwrap_or(false)
}

pub fn start(runtime: &ManagedLocalRuntimeConfig) -> Result<(), String> {
    let manifest = EngineManifest::shipped().map_err(|error| error.to_string())?;
    if runtime.kind != "hy-mt" {
        return Err(format!(
            "Unsupported managed runtime kind '{}'",
            runtime.kind
        ));
    }

    let executable = PathBuf::from(&runtime.executable_path);
    let model = PathBuf::from(&runtime.model_path);
    if !executable.is_file() {
        return Err(format!(
            "HY-MT runtime is missing: {}",
            executable.display()
        ));
    }
    if !model.is_file() {
        return Err(format!("HY-MT model is missing: {}", model.display()));
    }

    let log_dir = executable
        .parent()
        .ok_or_else(|| "HY-MT runtime path has no parent directory".to_string())?;
    let stdout = OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_dir.join("hy-mt-server.log"))
        .map_err(|error| format!("Failed to open HY-MT server log: {}", error))?;
    let stderr = OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_dir.join("hy-mt-server.err.log"))
        .map_err(|error| format!("Failed to open HY-MT error log: {}", error))?;

    let port = runtime.port.to_string();
    let mut command = Command::new(&executable);
    command
        .current_dir(log_dir)
        .args(["-m", &runtime.model_path])
        .args(["--alias", &manifest.model.id])
        .args(["--host", &manifest.launch.host])
        .args(["--port", &port])
        .args(["-c", &manifest.launch.context_size.to_string()])
        .args(["-ngl", &manifest.launch.gpu_layers.to_string()])
        .args(&manifest.launch.extra_args)
        .stdin(Stdio::null())
        .stdout(Stdio::from(stdout))
        .stderr(Stdio::from(stderr));

    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        command.creation_flags(CREATE_NO_WINDOW);
    }

    let mut owned = OWNED_RUNTIME
        .get_or_init(|| Mutex::new(None))
        .lock()
        .map_err(|_| "ENGINE_RUNTIME_LOCK_POISONED".to_string())?;
    if let Some(child) = owned.as_mut() {
        if child
            .try_wait()
            .map_err(|error| format!("ENGINE_RUNTIME_STATUS: {error}"))?
            .is_none()
        {
            return Ok(());
        }
        *owned = None;
    }

    let child = command
        .spawn()
        .map_err(|error| format!("Failed to start HY-MT runtime: {}", error))?;
    *owned = Some(child);
    Ok(())
}

/// Stop only the exact runtime child spawned by this app process.
pub fn shutdown_owned() {
    let Some(runtime) = OWNED_RUNTIME.get() else {
        return;
    };
    let Ok(mut owned) = runtime.lock() else {
        return;
    };
    if let Some(mut child) = owned.take() {
        let _ = child.kill();
        let _ = child.wait();
    }
}

pub fn owned_pid() -> Option<u32> {
    OWNED_RUNTIME
        .get()
        .and_then(|runtime| runtime.lock().ok())
        .and_then(|owned| owned.as_ref().map(Child::id))
}

pub async fn ensure_ready(
    runtime: &ManagedLocalRuntimeConfig,
    timeout: Duration,
) -> Result<String, String> {
    if is_healthy(runtime).await {
        return Ok(endpoint_url(runtime));
    }

    start(runtime)?;
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        sleep(Duration::from_millis(500)).await;
        if is_healthy(runtime).await {
            return Ok(endpoint_url(runtime));
        }
    }

    Err(format!(
        "HY-MT runtime did not become ready within {} seconds",
        timeout.as_secs()
    ))
}

pub fn start_configured(runtime: Option<ManagedLocalRuntimeConfig>) {
    let Some(runtime) = runtime else {
        return;
    };
    tauri::async_runtime::spawn(async move {
        match ensure_ready(&runtime, Duration::from_secs(90)).await {
            Ok(_) => info!("Local Translation Engine is ready"),
            Err(error) => warn!("Local Translation Engine startup failed: {}", error),
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn install_layout_is_stable_and_port_is_local_only() {
        let manifest = EngineManifest::shipped().expect("manifest should be valid");
        let package = manifest
            .runtime_for_current_arch()
            .expect("current architecture should be supported");
        let paths = HyMtInstallPaths::from_cache_root(r"D:\model-cache", &manifest, package);
        let runtime = paths.managed_config(&manifest);

        assert!(paths.model.ends_with(&manifest.model.artifact.file_name));
        assert!(paths.executable.ends_with("llama-server.exe"));
        assert_eq!(endpoint_url(&runtime), "http://127.0.0.1:11436");
        assert!(package.archive.size_bytes > 0);
        assert_eq!(package.archive.sha256.len(), 64);
    }

    #[test]
    fn release_asset_matches_current_windows_architecture() {
        let manifest = EngineManifest::shipped().expect("manifest should be valid");
        let package = manifest
            .runtime_for_current_arch()
            .expect("current architecture should be supported");
        assert!(package.archive.file_name.ends_with(".zip"));
        assert!(package.archive.url.contains(&package.archive.file_name));
        assert!(package.executable.relative_path.ends_with(".exe"));
    }
}
