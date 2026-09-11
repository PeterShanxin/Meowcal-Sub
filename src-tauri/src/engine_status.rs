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
use crate::llm::{FoundryLocalPhase, FoundryProbeSnapshot};
use engine_status_legacy::{legacy_prepare, legacy_refresh, legacy_status_no_probe};
use engine_status_make_ready::make_ready_legacy_http;

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
    managed_status(&config, false).await
}

pub async fn refresh_status_tauri(
    config: FoundryLocalConfig,
) -> Result<EngineStatusSnapshot, String> {
    managed_status(&config, false).await
}

pub async fn prepare_tauri(config: FoundryLocalConfig) -> Result<EngineStatusSnapshot, String> {
    managed_status(&config, true).await
}

pub async fn make_ready_tauri(config: FoundryLocalConfig) -> Result<EngineStatusSnapshot, String> {
    managed_status(&config, true).await
}

// ---------------------------------------------------------------------------
// HTTP profile (no managed branch; soft join fallback)
// ---------------------------------------------------------------------------

pub async fn get_status_http(config: FoundryLocalConfig) -> EngineStatusSnapshot {
    // Intentionally no managed_runtime branch — preserves current HTTP behavior.
    legacy_status_no_probe(config)
}

pub async fn refresh_status_http(config: FoundryLocalConfig) -> EngineStatusSnapshot {
    legacy_refresh(config).await
}

pub async fn prepare_http(config: FoundryLocalConfig) -> EngineStatusSnapshot {
    legacy_prepare(config).await
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
) -> Result<EngineStatusSnapshot, String> {
    let status = if start_if_needed {
        crate::core_client::ready(crate::core_client::READY_TIMEOUT).await?
    } else {
        match crate::core_client::status_if_idle().await? {
            crate::core_client::StatusPoll::Status(status) => *status,
            crate::core_client::StatusPoll::Busy => return Ok(preparing_snapshot(config)),
        }
    };
    Ok(managed_snapshot(config, status))
}

fn preparing_snapshot(config: &FoundryLocalConfig) -> EngineStatusSnapshot {
    EngineStatusSnapshot {
        cli_available: config.managed_runtime.is_some(),
        service_running: false,
        service_url: None,
        models: Vec::new(),
        configured_model: config.model.clone(),
        selected_model: None,
        notes: "Local translation engine is busy. Please wait for the current operation."
            .to_string(),
        phase: FoundryLocalPhase::Preparing,
        probe: None,
    }
}

fn managed_snapshot(
    config: &FoundryLocalConfig,
    status: crate::core_client::CoreStatus,
) -> EngineStatusSnapshot {
    let phase = if status.ready {
        FoundryLocalPhase::Ready
    } else if status.installed {
        FoundryLocalPhase::NotRunning
    } else {
        FoundryLocalPhase::NotInstalled
    };
    let notes = match phase {
        FoundryLocalPhase::Ready => "Local Translation Engine is ready.".to_string(),
        FoundryLocalPhase::NotInstalled => "Translation runtime is missing.".to_string(),
        FoundryLocalPhase::NotRunning => "Translation engine is installed but stopped.".to_string(),
        _ => "Local Translation Engine is configured.".to_string(),
    };

    EngineStatusSnapshot {
        cli_available: status.installed,
        service_running: status.ready,
        service_url: None,
        models: status
            .installed
            .then_some(status.model.clone())
            .into_iter()
            .collect(),
        configured_model: config.model.clone(),
        selected_model: status.installed.then_some(status.model),
        notes,
        phase,
        probe: None,
    }
}

#[cfg(test)]
#[path = "engine_status_tests.rs"]
mod tests;
