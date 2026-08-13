// Legacy Foundry CLI/probe orchestration used by engine_status profiles.

use super::EngineStatusSnapshot;
use crate::config::FoundryLocalConfig;
use crate::llm::{
    FoundryLocalBackend, FoundryLocalPhase, TranslatorBackend, FAST_PROBE_TIMEOUT_MS,
    SLOW_PROBE_TIMEOUT_MS,
};
use tracing::{debug, info, warn};

#[derive(Clone, Copy)]
pub(super) enum JoinPolicy {
    Hard,
    SoftRefresh,
    SoftPrepare,
}

#[derive(Clone, Copy)]
pub(super) enum PrepareNotes {
    Tauri,
    Http,
}

pub(super) fn legacy_status_no_probe(config: FoundryLocalConfig) -> EngineStatusSnapshot {
    let configured_model = config.model.clone();
    let backend = FoundryLocalBackend::new(config);
    backend.refresh_service_status();
    let cli_available = FoundryLocalBackend::is_cli_available();
    let service_url = FoundryLocalBackend::get_service_url_from_cli();
    let service_running = service_url.is_some();
    let models = if service_running {
        FoundryLocalBackend::get_cached_models_from_cli()
    } else {
        Vec::new()
    };
    let phase = backend.phase();

    EngineStatusSnapshot {
        cli_available,
        service_running,
        service_url,
        models,
        configured_model,
        selected_model: backend.selected_model(),
        notes: backend.notes(),
        phase,
        probe: backend.probe_snapshot(),
    }
}

pub(super) async fn legacy_refresh(
    config: FoundryLocalConfig,
    join: JoinPolicy,
) -> Result<EngineStatusSnapshot, String> {
    let configured_model = config.model.clone();
    let snapshot = tokio::task::spawn_blocking({
        let config = config.clone();
        move || legacy_blocking_snapshot(config, false)
    })
    .await;

    let (backend, cli_available, service_url, service_running, models, notes) = match snapshot {
        Ok(parts) => parts,
        Err(err) => match join {
            JoinPolicy::Hard => {
                return Err(format!("Engine status task failed: {err}"));
            }
            JoinPolicy::SoftRefresh | JoinPolicy::SoftPrepare => {
                let backend = FoundryLocalBackend::new(config);
                (
                    backend,
                    false,
                    None,
                    false,
                    Vec::new(),
                    "Engine refresh task failed".to_string(),
                )
            }
        },
    };

    let phase = probe_phase_fast(&backend, service_running, &models).await;

    Ok(EngineStatusSnapshot {
        cli_available,
        service_running,
        service_url,
        models,
        configured_model,
        selected_model: backend.selected_model(),
        notes,
        phase,
        probe: backend.probe_snapshot(),
    })
}

pub(super) async fn legacy_prepare(
    config: FoundryLocalConfig,
    join: JoinPolicy,
    notes_policy: PrepareNotes,
) -> Result<EngineStatusSnapshot, String> {
    let configured_model = config.model.clone();
    let snapshot = tokio::task::spawn_blocking({
        let config = config.clone();
        move || legacy_blocking_snapshot(config, true)
    })
    .await;

    let (backend, cli_available, service_url, service_running, models, mut notes) = match snapshot {
        Ok(parts) => parts,
        Err(err) => match join {
            JoinPolicy::Hard => {
                return Err(format!("Engine prepare task failed: {err}"));
            }
            JoinPolicy::SoftRefresh | JoinPolicy::SoftPrepare => {
                let backend = FoundryLocalBackend::new(config);
                (
                    backend,
                    false,
                    None,
                    false,
                    Vec::new(),
                    "Engine prepare task failed".to_string(),
                )
            }
        },
    };

    let phase = if service_running && !models.is_empty() {
        info!(
            "Starting Foundry Local warmup probe ({}ms timeout)",
            SLOW_PROBE_TIMEOUT_MS
        );
        match backend.probe_chat_completions(SLOW_PROBE_TIMEOUT_MS).await {
            Ok(true) => {
                info!("Foundry Local warmup probe succeeded");
                if matches!(notes_policy, PrepareNotes::Tauri) {
                    notes = format!("{notes} Warmup complete.");
                }
                FoundryLocalPhase::Ready
            }
            Ok(false) => {
                info!("Foundry Local warmup probe timed out (model still warming up)");
                if matches!(notes_policy, PrepareNotes::Tauri) {
                    notes = format!("{notes} Model still warming up.");
                }
                FoundryLocalPhase::Preparing
            }
            Err(e) => {
                warn!("Foundry Local warmup probe failed: {e}");
                notes = format!("{notes} Probe error: {e}");
                FoundryLocalPhase::Error
            }
        }
    } else {
        backend.phase()
    };

    Ok(EngineStatusSnapshot {
        cli_available,
        service_running,
        service_url,
        models,
        configured_model,
        selected_model: backend.selected_model(),
        notes,
        phase,
        probe: backend.probe_snapshot(),
    })
}

fn legacy_blocking_snapshot(
    config: FoundryLocalConfig,
    ensure_running: bool,
) -> (
    FoundryLocalBackend,
    bool,
    Option<String>,
    bool,
    Vec<String>,
    String,
) {
    let backend = FoundryLocalBackend::new(config);
    if ensure_running {
        backend.ensure_service_running();
    } else {
        backend.refresh_service_status();
    }
    let cli_available = FoundryLocalBackend::is_cli_available();
    let service_url = FoundryLocalBackend::get_service_url_from_cli();
    let service_running = service_url.is_some();
    let models = if service_running {
        FoundryLocalBackend::get_cached_models_from_cli()
    } else {
        Vec::new()
    };
    let notes = backend.notes();
    (
        backend,
        cli_available,
        service_url,
        service_running,
        models,
        notes,
    )
}

async fn probe_phase_fast(
    backend: &FoundryLocalBackend,
    service_running: bool,
    models: &[String],
) -> FoundryLocalPhase {
    if service_running && !models.is_empty() {
        if backend.is_probe_cache_valid() {
            debug!("Foundry Local probe cache valid, returning ready");
            FoundryLocalPhase::Ready
        } else {
            debug!(
                "Running fast Foundry Local probe ({}ms timeout)",
                FAST_PROBE_TIMEOUT_MS
            );
            match backend.probe_chat_completions(FAST_PROBE_TIMEOUT_MS).await {
                Ok(true) => {
                    info!("Foundry Local fast probe succeeded");
                    FoundryLocalPhase::Ready
                }
                Ok(false) => {
                    info!("Foundry Local fast probe timed out (model preparing)");
                    FoundryLocalPhase::Preparing
                }
                Err(e) => {
                    warn!("Foundry Local fast probe failed: {e}");
                    FoundryLocalPhase::Error
                }
            }
        }
    } else {
        backend.phase()
    }
}
