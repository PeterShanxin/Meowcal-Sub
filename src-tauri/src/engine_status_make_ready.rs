// Make-ready control flows. Tauri and HTTP keep separate loops on purpose
// (see Wave-1 parity matrix); do not merge them without a product decision.

use super::EngineStatusSnapshot;
use crate::config::FoundryLocalConfig;
use crate::llm::{
    FoundryLocalBackend, FoundryLocalPhase, TranslatorBackend, FAST_PROBE_TIMEOUT_MS,
    SLOW_PROBE_TIMEOUT_MS,
};
use std::sync::Arc;
use std::time::{Duration, Instant};

pub(super) async fn make_ready_legacy_tauri(
    config: FoundryLocalConfig,
) -> Result<EngineStatusSnapshot, String> {
    let configured_model = config.model.clone();
    let configured_timeout_ms = config.timeout_ms as u64;
    let steady_probe_timeout_ms = configured_timeout_ms.clamp(5_000, SLOW_PROBE_TIMEOUT_MS);
    let backend = Arc::new(FoundryLocalBackend::new(config));

    let started = Instant::now();
    let max_total = Duration::from_secs(90);

    let mut cli_available = false;
    let mut service_url: Option<String> = None;
    let mut service_running = false;
    let mut models: Vec<String> = Vec::new();
    let mut notes = String::new();
    let mut last_error: Option<String> = None;
    let mut phase = FoundryLocalPhase::Preparing;
    let mut attempt = 0usize;
    let mut models_wait_started: Option<Instant> = None;

    while started.elapsed() < max_total {
        let (snap_cli, snap_url, snap_running, snap_models, snap_notes) =
            tokio::task::spawn_blocking({
                let backend = backend.clone();
                move || {
                    backend.refresh_service_status();
                    let cli_available = FoundryLocalBackend::is_cli_available();
                    let service_url = FoundryLocalBackend::get_service_url_from_cli();
                    let service_running = service_url.is_some();
                    let models = if service_running {
                        FoundryLocalBackend::get_cached_models_from_cli()
                    } else {
                        Vec::new()
                    };
                    let notes = backend.notes();
                    (cli_available, service_url, service_running, models, notes)
                }
            })
            .await
            .map_err(|err| format!("Foundry Local make-ready snapshot failed: {err}"))?;

        cli_available = snap_cli;
        service_url = snap_url;
        service_running = snap_running;
        models = snap_models;
        notes = snap_notes;

        if !cli_available {
            phase = FoundryLocalPhase::NotInstalled;
            break;
        }

        if !service_running {
            phase = FoundryLocalPhase::NotRunning;
            let _ = tokio::task::spawn_blocking({
                let backend = backend.clone();
                move || backend.ensure_service_running()
            })
            .await;
            tokio::time::sleep(Duration::from_millis(900)).await;
            continue;
        }

        if models.is_empty() {
            phase = FoundryLocalPhase::NoModels;
            models_wait_started.get_or_insert_with(Instant::now);
            if models_wait_started
                .as_ref()
                .is_some_and(|t| t.elapsed() > Duration::from_secs(12))
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(900)).await;
            continue;
        }
        models_wait_started = None;

        attempt += 1;
        let timeout_ms = if attempt == 1 {
            SLOW_PROBE_TIMEOUT_MS
        } else {
            steady_probe_timeout_ms.max(FAST_PROBE_TIMEOUT_MS)
        };

        match backend.probe_chat_completions(timeout_ms).await {
            Ok(true) => {
                phase = FoundryLocalPhase::Ready;
                last_error = None;
                break;
            }
            Ok(false) => {
                phase = FoundryLocalPhase::Preparing;
            }
            Err(e) => {
                phase = FoundryLocalPhase::Error;
                last_error = Some(e.to_string());
            }
        }

        tokio::time::sleep(Duration::from_millis(1500)).await;
    }

    if phase != FoundryLocalPhase::Ready {
        if let Some(err) = last_error {
            notes = format!("{notes} Last error: {err}");
        }
    }

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

pub(super) async fn make_ready_legacy_http(config: FoundryLocalConfig) -> EngineStatusSnapshot {
    let configured_model = config.model.clone();
    let configured_timeout_ms = config.timeout_ms as u64;
    let steady_probe_timeout_ms = configured_timeout_ms.clamp(5_000, SLOW_PROBE_TIMEOUT_MS);

    let (backend, cli_available, mut service_url, mut service_running, mut models, mut notes) =
        match tokio::task::spawn_blocking({
            let config = config.clone();
            move || {
                let backend = FoundryLocalBackend::new(config);
                let cli_available = FoundryLocalBackend::is_cli_available();
                if cli_available {
                    backend.ensure_service_running();
                }
                backend.refresh_service_status();
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
        })
        .await
        {
            Ok(parts) => parts,
            Err(_) => {
                let backend = FoundryLocalBackend::new(config);
                (
                    backend,
                    false,
                    None,
                    false,
                    Vec::new(),
                    "Foundry Local make-ready task failed".to_string(),
                )
            }
        };

    if !cli_available || !service_running || models.is_empty() {
        let phase = backend.phase();
        return EngineStatusSnapshot {
            cli_available,
            service_running,
            service_url,
            models,
            configured_model,
            selected_model: backend.selected_model(),
            notes,
            phase,
            probe: backend.probe_snapshot(),
        };
    }

    let started = Instant::now();
    let max_total = Duration::from_secs(90);
    let mut attempt = 0usize;
    let mut phase = FoundryLocalPhase::Preparing;
    let mut last_error: Option<String> = None;

    while started.elapsed() < max_total {
        attempt += 1;
        let timeout_ms = if attempt == 1 {
            SLOW_PROBE_TIMEOUT_MS
        } else {
            steady_probe_timeout_ms.max(FAST_PROBE_TIMEOUT_MS)
        };

        match backend.probe_chat_completions(timeout_ms).await {
            Ok(true) => {
                phase = FoundryLocalPhase::Ready;
                last_error = None;
                break;
            }
            Ok(false) => {
                phase = FoundryLocalPhase::Preparing;
            }
            Err(e) => {
                phase = FoundryLocalPhase::Error;
                last_error = Some(e.to_string());
            }
        }

        backend.refresh_service_status();
        service_url = FoundryLocalBackend::get_service_url_from_cli();
        service_running = service_url.is_some();
        models = if service_running {
            FoundryLocalBackend::get_cached_models_from_cli()
        } else {
            Vec::new()
        };

        if !service_running {
            phase = FoundryLocalPhase::NotRunning;
            break;
        }

        tokio::time::sleep(Duration::from_millis(1500)).await;
    }

    if phase != FoundryLocalPhase::Ready {
        if let Some(err) = last_error {
            notes = format!("{notes} Last error: {err}");
        } else {
            notes = format!("{notes} Still warming up. Try again shortly.");
        }
    }

    EngineStatusSnapshot {
        cli_available,
        service_running,
        service_url,
        models,
        configured_model,
        selected_model: backend.selected_model(),
        notes,
        phase,
        probe: backend.probe_snapshot(),
    }
}
