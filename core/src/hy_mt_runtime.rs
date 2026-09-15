use crate::config::ManagedLocalRuntimeConfig;
use crate::engine_gpu_gate::{effective_launch_policy, LaunchPolicy};
use crate::engine_manifest::EngineManifest;
use reqwest::Client;
use std::io::Write;
use std::net::{Ipv4Addr, SocketAddrV4, TcpListener};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::Duration;
use tokio::time::Instant;
use tracing::info;

pub use crate::hy_mt_paths::HyMtInstallPaths;

static CPU_LOCKED: AtomicBool = AtomicBool::new(false);
pub fn lock_cpu() {
    CPU_LOCKED.store(true, Ordering::SeqCst);
}
pub fn cpu_locked() -> bool {
    CPU_LOCKED.load(Ordering::SeqCst)
}

static OWNED_RUNTIME: OnceLock<Mutex<Option<OwnedRuntime>>> = OnceLock::new();

// Measured GPU readiness: 3-6 s idle, ~11 s under load, 19 to 30+ s under memory pressure (#107).
const GPU_STARTUP_MAX: Duration = Duration::from_secs(45);
fn readiness_deadline(overall: Instant, now: Instant, gpu_active: bool) -> Instant {
    if !gpu_active {
        return overall;
    }
    now + std::cmp::min(GPU_STARTUP_MAX, overall.saturating_duration_since(now) / 2)
}

// Measured sample translations on the validated host: 0.2-2.2 s from a correct
// GPU engine, 2-3.2 s from a corrupt one running to its 120-token cap (#105).
const GPU_SAMPLE_MAX: Duration = Duration::from_secs(15);
/// When the sample on a healthy GPU engine must finish: the rest of the GPU
/// window, but never less than `GPU_SAMPLE_MAX`, and never past `overall`.
fn gpu_sample_deadline(overall: Instant, gpu_attempt: Instant, now: Instant) -> Instant {
    std::cmp::min(overall, std::cmp::max(gpu_attempt, now + GPU_SAMPLE_MAX))
}

#[derive(Debug)]
struct OwnedRuntime {
    child: Child,
    config: ManagedLocalRuntimeConfig,
    port: u16,
    acceleration: &'static str,
}

pub fn endpoint_url(runtime: &ManagedLocalRuntimeConfig) -> String {
    let port = active_owned_port(runtime).unwrap_or(runtime.port);
    format!("http://127.0.0.1:{port}")
}

pub async fn is_healthy(runtime: &ManagedLocalRuntimeConfig) -> bool {
    let Some(endpoint) = active_owned_endpoint(runtime) else {
        return false;
    };
    owned_runtime_is_healthy(runtime, &endpoint).await
}

async fn is_endpoint_healthy(endpoint: &str) -> bool {
    let Ok(client) = Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(2))
        .build()
    else {
        return false;
    };
    client
        .get(format!("{endpoint}/health"))
        .send()
        .await
        .map(|response| response.status().is_success())
        .unwrap_or(false)
}

async fn owned_runtime_is_healthy(runtime: &ManagedLocalRuntimeConfig, endpoint: &str) -> bool {
    if active_owned_endpoint(runtime).as_deref() != Some(endpoint) {
        return false;
    }
    let healthy = is_endpoint_healthy(endpoint).await;
    healthy && active_owned_endpoint(runtime).as_deref() == Some(endpoint)
}

/// The exact `llama-server` argument vector for a managed runtime, built pure
/// so the launch line is testable without spawning a process. Policy args are
/// appended last; manifest validation rejects any that name the Core-owned
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
        crate::engine_gpu_gate::adreno_gpu_allowed(),
        cpu_locked(),
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
            return Err("ENGINE_RUNTIME_BUSY: another Core-owned engine is running".to_string());
        }
        *owned = None;
    }

    #[cfg(target_os = "windows")]
    crate::process_lifetime::reap_orphans(&executable);
    let selected_port = select_loopback_port(runtime.port)?;
    let log_dir = executable
        .parent()
        .ok_or_else(|| "HY-MT runtime path has no parent directory".to_string())?;
    let port = selected_port.to_string();
    let mut command = Command::new(&executable);
    command
        .current_dir(log_dir)
        .args(launch_arguments(runtime, manifest, policy, &port))
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        // Runtime diagnostics must never enter the protocol's stdout pipe.
        .stderr(Stdio::inherit());

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
    let mut child = command
        .spawn()
        .map_err(|error| format!("Failed to start HY-MT runtime: {}", error))?;
    // `shutdown_owned` below only runs when the app exits cleanly. This is what
    // ends the engine when it does not - a crash, the installer replacing a
    // running app, Task Manager. See `process_lifetime`.
    #[cfg(target_os = "windows")]
    if let Err(error) = crate::process_lifetime::attach_to_app_lifetime(&child) {
        let _ = child.kill();
        let _ = child.wait();
        return Err(error);
    }
    *owned = Some(OwnedRuntime {
        child,
        config: runtime.clone(),
        port: selected_port,
        acceleration: if policy.gpu_layers > 0 { "gpu" } else { "cpu" },
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

pub fn owned_acceleration() -> Option<&'static str> {
    let mut owned = OWNED_RUNTIME.get()?.lock().ok()?;
    let active = owned.as_mut()?;
    match active.child.try_wait() {
        Ok(None) => Some(active.acceleration),
        Ok(Some(_)) => {
            *owned = None;
            None
        }
        Err(_) => None,
    }
}

#[path = "hy_mt_readiness.rs"]
mod readiness;
pub use readiness::ensure_ready;

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

#[cfg(test)]
#[path = "hy_mt_runtime_tests.rs"]
mod tests;
