use super::*;
use crate::config::{FoundryLocalConfig, ManagedLocalRuntimeConfig};
use crate::core_client::CoreStatus;
use crate::llm::{FoundryLocalPhase, SLOW_PROBE_TIMEOUT_MS};
use std::path::PathBuf;

fn managed_config() -> FoundryLocalConfig {
    FoundryLocalConfig {
        model: Some("legacy-model".to_string()),
        managed_runtime: Some(ManagedLocalRuntimeConfig {
            kind: "hy-mt".to_string(),
            executable_path: r"D:\old\server.exe".to_string(),
            model_path: r"D:\old\model.gguf".to_string(),
            port: 11_436,
        }),
        ..FoundryLocalConfig::default()
    }
}

fn core_status(installed: bool, ready: bool) -> CoreStatus {
    CoreStatus {
        installed,
        ready,
        model: "HY-MT1.5-1.8B-Q4_K_M".to_string(),
        version: "0.1.0".to_string(),
        storage_root: PathBuf::from(r"C:\core"),
        managed_config: None,
        install_paths: None,
    }
}

#[test]
fn managed_snapshot_uses_core_as_the_status_authority() {
    let config = managed_config();
    let stopped = managed_snapshot(&config, core_status(true, false));
    assert_eq!(stopped.phase, FoundryLocalPhase::NotRunning);
    assert!(stopped.cli_available);
    assert!(!stopped.service_running);
    assert!(stopped.service_url.is_none());
    assert_eq!(
        stopped.selected_model.as_deref(),
        Some("HY-MT1.5-1.8B-Q4_K_M")
    );

    let missing = managed_snapshot(&config, core_status(false, false));
    assert_eq!(missing.phase, FoundryLocalPhase::NotInstalled);
    assert!(!missing.cli_available);
    assert!(missing.models.is_empty());
}

#[tokio::test]
async fn http_get_status_ignores_managed_runtime_config() {
    let status = get_status_http(managed_config()).await;
    assert_ne!(status.notes, "Translation runtime is missing.");
}

#[test]
fn steady_probe_timeout_clamps_like_both_adapters() {
    let slow = SLOW_PROBE_TIMEOUT_MS;
    let clamp = |timeout_ms: u64| timeout_ms.clamp(5_000, slow);
    assert_eq!(clamp(1_000), 5_000);
    assert_eq!(clamp(10_000), 10_000.min(slow));
    assert_eq!(clamp(u64::MAX), slow);
}
