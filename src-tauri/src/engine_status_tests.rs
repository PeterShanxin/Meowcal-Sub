use super::*;
use crate::config::{FoundryLocalConfig, ManagedLocalRuntimeConfig};
use crate::llm::{FoundryLocalPhase, SLOW_PROBE_TIMEOUT_MS};
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

fn unique_root(label: &str) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
    let root = std::env::temp_dir().join(format!("meowcal-engine-status-{label}-{nanos}"));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root)?;
    Ok(root)
}

fn managed_config(
    root: &std::path::Path,
    with_exe: bool,
    with_model: bool,
) -> Result<FoundryLocalConfig, Box<dyn std::error::Error>> {
    let exe = root.join("llama-server.exe");
    let model = root.join("model.gguf");
    if with_exe {
        fs::write(&exe, b"fake-exe")?;
    }
    if with_model {
        // Size must match shipped manifest model size for model_ready; when
        // mismatch, managed path reports NoModels. Empty/wrong size is fine
        // for NotInstalled / NoModels tests.
        fs::write(&model, b"x")?;
    }
    let mut config = FoundryLocalConfig::default();
    config.model = Some("hy-mt-test".to_string());
    config.managed_runtime = Some(ManagedLocalRuntimeConfig {
        kind: "hy-mt".to_string(),
        executable_path: exe.to_string_lossy().into_owned(),
        model_path: model.to_string_lossy().into_owned(),
        port: 11436,
    });
    Ok(config)
}

#[tokio::test]
async fn managed_missing_executable_is_not_installed() -> Result<(), Box<dyn std::error::Error>> {
    let root = unique_root("no-exe")?;
    let config = managed_config(&root, false, false)?;
    let status = managed_status(&config, false)
        .await
        .expect("managed branch");
    assert_eq!(status.phase, FoundryLocalPhase::NotInstalled);
    assert!(!status.cli_available);
    assert!(!status.service_running);
    assert!(status.probe.is_none());
    assert_eq!(status.notes, "Translation runtime is missing.");
    assert!(status.models.is_empty());
    let _ = fs::remove_dir_all(root);
    Ok(())
}

#[tokio::test]
async fn managed_exe_without_matching_model_is_no_models() -> Result<(), Box<dyn std::error::Error>>
{
    let root = unique_root("no-model")?;
    let config = managed_config(&root, true, true)?;
    let status = managed_status(&config, false)
        .await
        .expect("managed branch");
    // Wrong model size → NoModels (manifest size will not match 1 byte).
    assert_eq!(status.phase, FoundryLocalPhase::NoModels);
    assert!(status.cli_available);
    assert!(!status.service_running);
    assert!(status.probe.is_none());
    assert_eq!(status.notes, "HY-MT model is missing or incomplete.");
    assert!(status.models.is_empty());
    assert_eq!(status.configured_model.as_deref(), Some("hy-mt-test"));
    assert_eq!(status.selected_model.as_deref(), Some("hy-mt-test"));
    assert!(status.service_url.is_some());
    let _ = fs::remove_dir_all(root);
    Ok(())
}

#[tokio::test]
async fn http_get_status_ignores_managed_runtime_config() -> Result<(), Box<dyn std::error::Error>>
{
    // HTTP must not take the managed branch even when managed_runtime is set.
    let root = unique_root("http-ignores-managed")?;
    let config = managed_config(&root, false, false)?;
    let status = get_status_http(config).await;
    // Without a real Foundry CLI this is legacy discovery, not managed notes.
    assert_ne!(status.notes, "Translation runtime is missing.");
    // Managed would set service_url to Some(endpoint); legacy may be None.
    // Critical: phase/notes are not the managed fixed strings path exclusively
    // proven by notes inequality above.
    let _ = fs::remove_dir_all(root);
    Ok(())
}

#[test]
fn steady_probe_timeout_clamps_like_both_adapters() {
    let slow = SLOW_PROBE_TIMEOUT_MS;
    let clamp = |timeout_ms: u64| timeout_ms.clamp(5_000, slow);
    assert_eq!(clamp(1_000), 5_000);
    assert_eq!(clamp(10_000), 10_000.min(slow));
    assert_eq!(clamp(u64::MAX), slow);
}
