// =============================================================================
// ENGINE_STATUS.RS - Engine readiness orchestration (#32 Wave 1)
// =============================================================================
// Owns status / refresh / prepare / make-ready orchestration for the local
// translation engine. Adapters map EngineStatusSnapshot to Foundry-named wire
// DTOs and must keep documented Tauri↔HTTP differences (see
// docs/superpowers/specs/2026-08-10-32-engine-status-wave1-design.md).
// =============================================================================

#[path = "engine_status_legacy.rs"]
mod engine_status_legacy;
#[path = "engine_status_make_ready.rs"]
mod engine_status_make_ready;

use crate::config::FoundryLocalConfig;
use crate::hy_mt_runtime;
use crate::llm::{FoundryLocalPhase, FoundryProbeSnapshot};
use engine_status_legacy::{
    legacy_prepare, legacy_refresh, legacy_status_no_probe, JoinPolicy, PrepareNotes,
};
use engine_status_make_ready::{make_ready_legacy_http, make_ready_legacy_tauri};
use std::path::PathBuf;
use std::time::Duration;
use tracing::warn;

/// Domain snapshot of engine readiness. Not a public IPC/HTTP wire DTO.
#[derive(Debug, Clone)]
pub struct EngineStatusSnapshot {
    pub cli_available: bool,
    pub service_running: bool,
    pub service_url: Option<String>,
    pub models: Vec<String>,
    pub configured_model: Option<String>,
    pub selected_model: Option<String>,
    pub notes: String,
    pub phase: FoundryLocalPhase,
    pub probe: Option<FoundryProbeSnapshot>,
}

// ---------------------------------------------------------------------------
// Tauri profile (managed branch enabled; hard join errors)
// ---------------------------------------------------------------------------

pub async fn get_status_tauri(config: FoundryLocalConfig) -> Result<EngineStatusSnapshot, String> {
    if let Some(status) = managed_status(&config, false).await {
        return Ok(status);
    }
    tokio::task::spawn_blocking(move || legacy_status_no_probe(config))
        .await
        .map_err(|err| {
            let message = format!("Foundry Local status task failed: {err}");
            warn!("{}", message);
            message
        })
}

pub async fn refresh_status_tauri(
    config: FoundryLocalConfig,
) -> Result<EngineStatusSnapshot, String> {
    if let Some(status) = managed_status(&config, false).await {
        return Ok(status);
    }
    legacy_refresh(config, JoinPolicy::Hard).await
}

pub async fn prepare_tauri(config: FoundryLocalConfig) -> Result<EngineStatusSnapshot, String> {
    if let Some(status) = managed_status(&config, true).await {
        return Ok(status);
    }
    legacy_prepare(config, JoinPolicy::Hard, PrepareNotes::Tauri).await
}

pub async fn make_ready_tauri(config: FoundryLocalConfig) -> Result<EngineStatusSnapshot, String> {
    if let Some(status) = managed_status(&config, true).await {
        return Ok(status);
    }
    make_ready_legacy_tauri(config).await
}

// ---------------------------------------------------------------------------
// HTTP profile (no managed branch; soft join fallback)
// ---------------------------------------------------------------------------

pub async fn get_status_http(config: FoundryLocalConfig) -> EngineStatusSnapshot {
    // Intentionally no managed_runtime branch — preserves current HTTP behavior.
    legacy_status_no_probe(config)
}

pub async fn refresh_status_http(config: FoundryLocalConfig) -> EngineStatusSnapshot {
    match legacy_refresh(config, JoinPolicy::SoftRefresh).await {
        Ok(status) => status,
        Err(_) => unreachable!("soft refresh never returns Err"),
    }
}

pub async fn prepare_http(config: FoundryLocalConfig) -> EngineStatusSnapshot {
    match legacy_prepare(config, JoinPolicy::SoftPrepare, PrepareNotes::Http).await {
        Ok(status) => status,
        Err(_) => unreachable!("soft prepare never returns Err"),
    }
}

pub async fn make_ready_http(config: FoundryLocalConfig) -> EngineStatusSnapshot {
    make_ready_legacy_http(config).await
}

// ---------------------------------------------------------------------------
// Managed runtime (Tauri path only today)
// ---------------------------------------------------------------------------

async fn managed_status(
    config: &FoundryLocalConfig,
    start_if_needed: bool,
) -> Option<EngineStatusSnapshot> {
    let runtime = config.managed_runtime.as_ref()?;
    let executable_ready = PathBuf::from(&runtime.executable_path).is_file();
    let expected_model_size = crate::engine_manifest::EngineManifest::shipped()
        .map(|manifest| manifest.model.artifact.size_bytes)
        .unwrap_or_default();
    let model_ready = PathBuf::from(&runtime.model_path)
        .metadata()
        .map(|metadata| expected_model_size > 0 && metadata.len() == expected_model_size)
        .unwrap_or(false);
    let service_running = if executable_ready && model_ready {
        if start_if_needed {
            hy_mt_runtime::ensure_ready(runtime, Duration::from_secs(90))
                .await
                .is_ok()
        } else {
            hy_mt_runtime::is_healthy(runtime).await
        }
    } else {
        false
    };
    let phase = if !executable_ready {
        FoundryLocalPhase::NotInstalled
    } else if !model_ready {
        FoundryLocalPhase::NoModels
    } else if service_running {
        FoundryLocalPhase::Ready
    } else {
        FoundryLocalPhase::NotRunning
    };
    let notes = match phase {
        FoundryLocalPhase::Ready => "Local Translation Engine is ready.".to_string(),
        FoundryLocalPhase::NotInstalled => "Translation runtime is missing.".to_string(),
        FoundryLocalPhase::NoModels => "HY-MT model is missing or incomplete.".to_string(),
        FoundryLocalPhase::NotRunning => "Translation engine is installed but stopped.".to_string(),
        _ => "Local Translation Engine is configured.".to_string(),
    };

    Some(EngineStatusSnapshot {
        cli_available: executable_ready,
        service_running,
        service_url: Some(hy_mt_runtime::endpoint_url(runtime)),
        models: config
            .model
            .clone()
            .into_iter()
            .filter(|_| model_ready)
            .collect(),
        configured_model: config.model.clone(),
        selected_model: config.model.clone(),
        notes,
        phase,
        probe: None,
    })
}

#[cfg(test)]
#[path = "engine_status_tests.rs"]
mod tests;
