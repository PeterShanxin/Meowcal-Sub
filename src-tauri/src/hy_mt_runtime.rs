use crate::config::ManagedLocalRuntimeConfig;
use reqwest::Client;
use std::fs::OpenOptions;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{Mutex, OnceLock};
use std::time::Duration;
use tokio::time::{sleep, Instant};
use tracing::{info, warn};

pub const HY_MT_MODEL_ID: &str = "HY-MT1.5-1.8B-Q4_K_M";
pub const HY_MT_MODEL_FILE: &str = "HY-MT1.5-1.8B-Q4_K_M.gguf";
pub const HY_MT_MODEL_SIZE: u64 = 1_133_080_512;
pub const HY_MT_MODEL_SHA256: &str =
    "4383ac0c3c8e476de98ff979c2a3f069f8c4fb385e7860cf2d28da896cc477c7";
pub const HY_MT_MODEL_URL: &str =
    "https://huggingface.co/tencent/HY-MT1.5-1.8B-GGUF/resolve/main/HY-MT1.5-1.8B-Q4_K_M.gguf";
pub const HY_MT_PORT: u16 = 11_436;
pub const LLAMA_RUNTIME_VERSION: &str = "b10155";
static OWNED_RUNTIME: OnceLock<Mutex<Option<Child>>> = OnceLock::new();

#[cfg(target_arch = "aarch64")]
pub const LLAMA_RUNTIME_ASSET: &str = "llama-b10155-bin-win-opencl-adreno-arm64.zip";
#[cfg(target_arch = "aarch64")]
pub const LLAMA_RUNTIME_SIZE: u64 = 12_868_798;
#[cfg(target_arch = "aarch64")]
pub const LLAMA_RUNTIME_SHA256: &str =
    "1b0ead2eec5489574dd6889390ee24c6c0cd8bd20d68cd4f6a28930c7ada9b07";
#[cfg(target_arch = "x86_64")]
pub const LLAMA_RUNTIME_ASSET: &str = "llama-b10155-bin-win-vulkan-x64.zip";
#[cfg(target_arch = "x86_64")]
pub const LLAMA_RUNTIME_SIZE: u64 = 33_576_473;
#[cfg(target_arch = "x86_64")]
pub const LLAMA_RUNTIME_SHA256: &str =
    "d9d6c72ab8922123b7fb040b4178105e96f15e296cc4b6c3153b938a1c7ff0b4";
#[cfg(not(any(target_arch = "aarch64", target_arch = "x86_64")))]
pub const LLAMA_RUNTIME_ASSET: &str = "";
#[cfg(not(any(target_arch = "aarch64", target_arch = "x86_64")))]
pub const LLAMA_RUNTIME_SIZE: u64 = 0;
#[cfg(not(any(target_arch = "aarch64", target_arch = "x86_64")))]
pub const LLAMA_RUNTIME_SHA256: &str = "";

pub fn llama_runtime_url() -> String {
    format!(
        "https://github.com/ggml-org/llama.cpp/releases/download/{}/{}",
        LLAMA_RUNTIME_VERSION, LLAMA_RUNTIME_ASSET
    )
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
    pub fn from_cache_root(cache_root: impl AsRef<Path>) -> Self {
        let root = cache_root.as_ref().join("meowcal-sub");
        #[cfg(target_arch = "aarch64")]
        let runtime_folder = format!("llama-{}-opencl-adreno-arm64", LLAMA_RUNTIME_VERSION);
        #[cfg(target_arch = "x86_64")]
        let runtime_folder = format!("llama-{}-vulkan-x64", LLAMA_RUNTIME_VERSION);
        #[cfg(not(any(target_arch = "aarch64", target_arch = "x86_64")))]
        let runtime_folder = format!("llama-{}-unsupported", LLAMA_RUNTIME_VERSION);
        let runtime_dir = root.join("runtime").join(runtime_folder);
        let model_dir = root.join("models").join("hy-mt1.5-1.8b-q4");
        Self {
            runtime_archive: root.join("runtime").join(LLAMA_RUNTIME_ASSET),
            executable: runtime_dir.join("llama-server.exe"),
            model: model_dir.join(HY_MT_MODEL_FILE),
            root,
            runtime_dir,
            model_dir,
        }
    }

    pub fn is_complete(&self) -> bool {
        self.executable.is_file()
            && self
                .model
                .metadata()
                .map(|metadata| metadata.len() == HY_MT_MODEL_SIZE)
                .unwrap_or(false)
    }

    pub fn managed_config(&self) -> ManagedLocalRuntimeConfig {
        ManagedLocalRuntimeConfig {
            kind: "hy-mt".to_string(),
            executable_path: self.executable.to_string_lossy().to_string(),
            model_path: self.model.to_string_lossy().to_string(),
            port: HY_MT_PORT,
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
        .args([
            "-m",
            &runtime.model_path,
            "--alias",
            HY_MT_MODEL_ID,
            "--host",
            "127.0.0.1",
            "--port",
            &port,
            "-c",
            "2048",
            "-ngl",
            "99",
            "--jinja",
            "--no-webui",
        ])
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
        let paths = HyMtInstallPaths::from_cache_root(r"D:\model-cache");
        let runtime = paths.managed_config();

        assert!(paths.model.ends_with(HY_MT_MODEL_FILE));
        assert!(paths.executable.ends_with("llama-server.exe"));
        assert_eq!(endpoint_url(&runtime), "http://127.0.0.1:11436");
        assert!(LLAMA_RUNTIME_SIZE > 0);
        assert_eq!(LLAMA_RUNTIME_SHA256.len(), 64);
    }

    #[test]
    fn release_asset_matches_current_windows_architecture() {
        assert!(!LLAMA_RUNTIME_ASSET.is_empty());
        assert!(LLAMA_RUNTIME_ASSET.ends_with(".zip"));
        assert!(llama_runtime_url().contains(LLAMA_RUNTIME_VERSION));
    }
}
