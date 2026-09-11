use crate::config::ManagedLocalRuntimeConfig;
use std::time::Duration;
use tracing::{info, warn};

pub use crate::hy_mt_paths::HyMtInstallPaths;

/// Compatibility value for persisted pre-Core configuration. Managed
/// translations never connect to this endpoint; they use `core_client` stdio.
pub fn endpoint_url(runtime: &ManagedLocalRuntimeConfig) -> String {
    format!("http://127.0.0.1:{}", runtime.port)
}

pub async fn is_healthy(_runtime: &ManagedLocalRuntimeConfig) -> bool {
    crate::core_client::status()
        .await
        .map(|status| status.ready)
        .unwrap_or(false)
}

pub fn start(_runtime: &ManagedLocalRuntimeConfig) -> Result<String, String> {
    let status = crate::core_client::ready_blocking(Duration::from_secs(90))?;
    if !status.ready {
        return Err("ENGINE_NOT_READY".to_string());
    }
    Ok("core://local-engine".to_string())
}

pub async fn ensure_ready(
    _runtime: &ManagedLocalRuntimeConfig,
    timeout: Duration,
) -> Result<String, String> {
    let status = crate::core_client::ready(timeout).await?;
    if !status.ready {
        return Err("ENGINE_NOT_READY".to_string());
    }
    Ok("core://local-engine".to_string())
}

pub fn shutdown_owned() {
    crate::core_client::shutdown_owned();
}

pub fn owned_pid() -> Option<u32> {
    crate::core_client::owned_pid()
}

pub fn start_configured(runtime: Option<ManagedLocalRuntimeConfig>) {
    if runtime.is_none() {
        return;
    }
    tauri::async_runtime::spawn(async move {
        match crate::core_client::ready(Duration::from_secs(90)).await {
            Ok(status) if status.ready => info!("Local Translation Engine is ready"),
            Ok(_) => warn!("Local Translation Engine did not report ready"),
            Err(error) => warn!("Local Translation Engine startup failed: {error}"),
        }
    });
}

#[cfg(test)]
#[path = "hy_mt_runtime_tests.rs"]
mod tests;
