use crate::config::ManagedLocalRuntimeConfig;
use crate::engine_manifest::{EngineManifest, RuntimeSpec};
use reqwest::Client;
use std::fs::OpenOptions;
use std::net::{Ipv4Addr, SocketAddrV4, TcpListener};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{Mutex, OnceLock};
use std::time::Duration;
use tokio::time::{sleep, timeout_at, Instant};
use tracing::{info, warn};

pub use crate::hy_mt_paths::HyMtInstallPaths;

static OWNED_RUNTIME: OnceLock<Mutex<Option<OwnedRuntime>>> = OnceLock::new();

// Measured GPU readiness: 3-6 seconds, about 11 seconds under ambient load.
const GPU_STARTUP_MAX: Duration = Duration::from_secs(30);
fn readiness_deadline(overall: Instant, now: Instant, gpu_active: bool) -> Instant {
    if !gpu_active {
        return overall;
    }
    now + std::cmp::min(GPU_STARTUP_MAX, overall.saturating_duration_since(now) / 2)
}

#[derive(Debug)]
struct OwnedRuntime {
    child: Child,
    config: ManagedLocalRuntimeConfig,
    port: u16,
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

/// The acceleration policy a launch actually runs with. The manifest asks;
/// this decides: the Adreno GPU policy applies only on the validated GPU
/// (`engine_gpu_gate`) and can be forced off for the startup fallback.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LaunchPolicy {
    pub gpu_layers: u32,
    pub launch_args: Vec<String>,
    /// Whether this policy puts layers on the GPU. Drives the one-shot CPU
    /// retry in `ensure_ready`: only a GPU attempt earns a fallback.
    pub gpu_active: bool,
}

/// The effective policy for a runtime on this host. Three outcomes:
///
/// - not the Adreno runtime (e.g. x64 Vulkan), or it requests no layers:
///   the manifest policy exactly as shipped;
/// - the Adreno runtime on the validated GPU, not forced off: the benchmarked
///   `-ngl 99 --no-kv-offload` configuration;
/// - the Adreno runtime anywhere else, or forced off after a failed GPU
///   start: the pre-GPU CPU policy (`-ngl 0`, no KV flag - the flag only
///   constrains GPU KV offload, and the fallback line should be exactly what
///   CPU-only releases ran).
pub(crate) fn effective_launch_policy(
    runtime_spec: &RuntimeSpec,
    adreno_gpu_validated: bool,
    force_cpu: bool,
) -> LaunchPolicy {
    let adreno_gpu_requested = runtime_spec.id == crate::engine_manifest::ADRENO_B10155_RUNTIME_ID
        && runtime_spec.gpu_layers > 0;
    if adreno_gpu_requested && (force_cpu || !adreno_gpu_validated) {
        return LaunchPolicy {
            gpu_layers: 0,
            launch_args: Vec::new(),
            gpu_active: false,
        };
    }
    LaunchPolicy {
        gpu_layers: runtime_spec.gpu_layers,
        launch_args: runtime_spec.launch_args.clone(),
        gpu_active: adreno_gpu_requested,
    }
}

/// The exact `llama-server` argument vector for a managed runtime, built pure
/// so the launch line is testable without spawning a process. Policy args are
/// appended last; manifest validation rejects any that name the app-owned
/// flags above them (see `engine_launch`).
pub(crate) fn launch_arguments(
    runtime: &ManagedLocalRuntimeConfig,
    manifest: &EngineManifest,
    policy: &LaunchPolicy,
    port: &str,
) -> Vec<String> {
    let mut arguments = vec![
        "-m".to_string(),
        runtime.model_path.clone(),
        "--alias".to_string(),
        manifest.model.id.clone(),
        "--host".to_string(),
        manifest.launch.host.clone(),
        "--port".to_string(),
        port.to_string(),
        "-c".to_string(),
        manifest.launch.context_size.to_string(),
        "-ngl".to_string(),
        policy.gpu_layers.to_string(),
    ];
    arguments.extend(crate::engine_launch::launch_args(
        &manifest.launch.extra_args,
        crate::engine_launch::available_cores(),
    ));
    arguments.extend(policy.launch_args.iter().cloned());
    arguments
}

pub fn start(runtime: &ManagedLocalRuntimeConfig) -> Result<String, String> {
    let manifest = EngineManifest::shipped().map_err(|error| error.to_string())?;
    let runtime_spec = manifest
        .runtime_for_current_arch()
        .map_err(|error| error.to_string())?;
    let policy = effective_launch_policy(
        runtime_spec,
        crate::engine_gpu_gate::validated_adreno_gpu_present(),
        false,
    );
    start_with_policy(runtime, &manifest, &policy)
}

fn start_with_policy(
    runtime: &ManagedLocalRuntimeConfig,
    manifest: &EngineManifest,
    policy: &LaunchPolicy,
) -> Result<String, String> {
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
        .args(launch_arguments(runtime, manifest, policy, &port))
        .stdin(Stdio::null())
        .stdout(Stdio::from(stdout))
        .stderr(Stdio::from(stderr));

    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        command.creation_flags(CREATE_NO_WINDOW);
    }

    info!(
        "HY-MT launch policy: {}",
        if policy.gpu_active {
            "Adreno GPU (validated host, KV cache on CPU)"
        } else if policy.gpu_layers == 0 {
            "CPU"
        } else {
            "manifest acceleration"
        }
    );
    let child = command
        .spawn()
        .map_err(|error| format!("Failed to start HY-MT runtime: {}", error))?;
    // `shutdown_owned` below only runs when the app exits cleanly. This is what
    // ends the engine when it does not - a crash, the installer replacing a
    // running app, Task Manager. See `process_lifetime`.
    #[cfg(target_os = "windows")]
    crate::process_lifetime::attach_to_app_lifetime(&child);
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
    ensure_ready_with_policy(runtime, Instant::now() + timeout, timeout, false).await
}
/// GPU readiness failure retries on CPU within one deadline; post-ready wedges remain #103.
async fn ensure_ready_with_policy(
    runtime: &ManagedLocalRuntimeConfig,
    deadline: Instant,
    timeout: Duration,
    force_cpu: bool,
) -> Result<String, String> {
    let timeout_error = || {
        format!(
            "HY-MT runtime did not become ready within {} seconds",
            timeout.as_secs()
        )
    };
    if !force_cpu {
        if let Some(endpoint) = active_owned_endpoint(runtime) {
            let healthy = timeout_at(deadline, is_endpoint_healthy(&endpoint))
                .await
                .unwrap_or(false);
            if healthy {
                return Ok(endpoint);
            }
        }
    }
    if Instant::now() >= deadline {
        return Err(timeout_error());
    }
    let manifest = EngineManifest::shipped().map_err(|error| error.to_string())?;
    let runtime_spec = manifest
        .runtime_for_current_arch()
        .map_err(|error| error.to_string())?;
    let policy = effective_launch_policy(
        runtime_spec,
        crate::engine_gpu_gate::validated_adreno_gpu_present(),
        force_cpu,
    );
    let endpoint = start_with_policy(runtime, &manifest, &policy)?;
    let attempt_started = Instant::now();
    let attempt_deadline = readiness_deadline(deadline, attempt_started, policy.gpu_active);
    let healthy = timeout_at(attempt_deadline, async {
        loop {
            sleep(Duration::from_millis(500)).await;
            if is_endpoint_healthy(&endpoint).await {
                return;
            }
        }
    })
    .await
    .is_ok();
    if healthy {
        return Ok(endpoint);
    }
    if policy.gpu_active {
        let gpu_window = attempt_deadline.saturating_duration_since(attempt_started);
        warn!(
            "HY-MT GPU engine did not become ready within {} seconds; retrying on CPU",
            gpu_window.as_secs()
        );
        shutdown_owned();
        return Box::pin(ensure_ready_with_policy(runtime, deadline, timeout, true)).await;
    }
    Err(timeout_error())
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

    // Engines stranded by app versions that shipped before the job object, or
    // by anything the job object cannot cover. Runs before we start our own, so
    // there is no chance of reaping it, and each one freed is a model's worth
    // of memory the machine gets back.
    #[cfg(target_os = "windows")]
    {
        let reaped = crate::process_lifetime::reap_orphans(Path::new(&runtime.executable_path));
        if reaped > 0 {
            warn!("Cleaned up {reaped} translation engine(s) left behind by an earlier run");
        }
    }

    tauri::async_runtime::spawn(async move {
        match ensure_ready(&runtime, Duration::from_secs(90)).await {
            Ok(_) => info!("Local Translation Engine is ready"),
            Err(error) => warn!("Local Translation Engine startup failed: {}", error),
        }
    });
}

#[cfg(test)]
#[path = "hy_mt_runtime_tests.rs"]
mod tests;
