use crate::config::ManagedLocalRuntimeConfig;
use crate::engine_manifest::{EngineManifest, RuntimeSpec};
use reqwest::Client;
use std::fs::OpenOptions;
use std::net::{Ipv4Addr, SocketAddrV4, TcpListener};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{Mutex, OnceLock};
use std::time::Duration;
use tokio::time::{sleep, Instant};
use tracing::{info, warn};

static OWNED_RUNTIME: OnceLock<Mutex<Option<OwnedRuntime>>> = OnceLock::new();

#[derive(Debug)]
struct OwnedRuntime {
    child: Child,
    config: ManagedLocalRuntimeConfig,
    port: u16,
}

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
    let port = active_owned_port(runtime).unwrap_or(runtime.port);
    format!("http://127.0.0.1:{port}")
}

pub async fn is_healthy(runtime: &ManagedLocalRuntimeConfig) -> bool {
    let Some(endpoint) = active_owned_endpoint(runtime) else {
        return false;
    };
    is_endpoint_healthy(&endpoint).await
}

async fn is_endpoint_healthy(endpoint: &str) -> bool {
    let client = Client::builder()
        .timeout(Duration::from_secs(2))
        .build()
        .unwrap_or_default();
    client
        .get(format!("{endpoint}/health"))
        .send()
        .await
        .map(|response| response.status().is_success())
        .unwrap_or(false)
}

pub fn start(runtime: &ManagedLocalRuntimeConfig) -> Result<String, String> {
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

    let mut owned = OWNED_RUNTIME
        .get_or_init(|| Mutex::new(None))
        .lock()
        .map_err(|_| "ENGINE_RUNTIME_LOCK_POISONED".to_string())?;
    if let Some(active) = owned.as_mut() {
        let running = active
            .child
            .try_wait()
            .map_err(|error| format!("ENGINE_RUNTIME_STATUS: {error}"))?
            .is_none();
        if running && same_runtime(&active.config, runtime) {
            return Ok(format!("http://127.0.0.1:{}", active.port));
        }
        if running {
            return Err("ENGINE_RUNTIME_BUSY: another app-owned engine is running".to_string());
        }
        *owned = None;
    }

    let selected_port = select_loopback_port(runtime.port)?;
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

    let port = selected_port.to_string();
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

    let child = command
        .spawn()
        .map_err(|error| format!("Failed to start HY-MT runtime: {}", error))?;
    *owned = Some(OwnedRuntime {
        child,
        config: runtime.clone(),
        port: selected_port,
    });
    Ok(format!("http://127.0.0.1:{selected_port}"))
}

/// Stop only the exact runtime child spawned by this app process.
pub fn shutdown_owned() {
    let Some(runtime) = OWNED_RUNTIME.get() else {
        return;
    };
    let Ok(mut owned) = runtime.lock() else {
        return;
    };
    if let Some(mut active) = owned.take() {
        let _ = active.child.kill();
        let _ = active.child.wait();
    }
}

pub fn owned_pid() -> Option<u32> {
    OWNED_RUNTIME
        .get()
        .and_then(|runtime| runtime.lock().ok())
        .and_then(|owned| owned.as_ref().map(|active| active.child.id()))
}

pub async fn ensure_ready(
    runtime: &ManagedLocalRuntimeConfig,
    timeout: Duration,
) -> Result<String, String> {
    if let Some(endpoint) = active_owned_endpoint(runtime) {
        if is_endpoint_healthy(&endpoint).await {
            return Ok(endpoint);
        }
    }

    let endpoint = start(runtime)?;
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        sleep(Duration::from_millis(500)).await;
        if is_endpoint_healthy(&endpoint).await {
            return Ok(endpoint);
        }
    }

    Err(format!(
        "HY-MT runtime did not become ready within {} seconds",
        timeout.as_secs()
    ))
}

fn active_owned_endpoint(runtime: &ManagedLocalRuntimeConfig) -> Option<String> {
    active_owned_port(runtime).map(|port| format!("http://127.0.0.1:{port}"))
}

fn active_owned_port(runtime: &ManagedLocalRuntimeConfig) -> Option<u16> {
    let mut owned = OWNED_RUNTIME.get()?.lock().ok()?;
    let active = owned.as_mut()?;
    match active.child.try_wait() {
        Ok(None) if same_runtime(&active.config, runtime) => Some(active.port),
        Ok(Some(_)) => {
            *owned = None;
            None
        }
        _ => None,
    }
}

fn same_runtime(left: &ManagedLocalRuntimeConfig, right: &ManagedLocalRuntimeConfig) -> bool {
    left.kind == right.kind
        && left.executable_path == right.executable_path
        && left.model_path == right.model_path
}

fn select_loopback_port(preferred: u16) -> Result<u16, String> {
    if preferred != 0
        && TcpListener::bind(SocketAddrV4::new(Ipv4Addr::LOCALHOST, preferred)).is_ok()
    {
        return Ok(preferred);
    }
    let listener = TcpListener::bind(SocketAddrV4::new(Ipv4Addr::LOCALHOST, 0))
        .map_err(|error| format!("ENGINE_PORT_SELECTION: {error}"))?;
    listener
        .local_addr()
        .map(|address| address.port())
        .map_err(|error| format!("ENGINE_PORT_SELECTION: {error}"))
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

    #[test]
    fn occupied_preferred_port_selects_another_loopback_port() {
        let occupied =
            TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).expect("fixture listener should bind");
        let preferred = occupied
            .local_addr()
            .expect("fixture address should be available")
            .port();
        let selected = select_loopback_port(preferred).expect("fallback port should be selected");

        assert_ne!(selected, preferred);
        assert!(TcpListener::bind((Ipv4Addr::LOCALHOST, preferred)).is_err());
    }
}
