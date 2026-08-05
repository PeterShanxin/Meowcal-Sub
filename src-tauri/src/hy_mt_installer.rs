use crate::config::FoundryLocalConfig;
use crate::engine_artifact_io::{
    download_file, extract_zip, file_matches, verify_download, verify_executable,
};
use crate::engine_install_transaction::{
    promote_assets, record_active, recover_active, recover_pending_asset, reset_candidate,
    InstalledEngine,
};
use crate::engine_manifest::EngineManifest;
use crate::hy_mt_runtime::HyMtInstallPaths;
use crate::llm::{FoundryLocalBackend, TranslatorBackend};
use std::path::{Path, PathBuf};
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager, Runtime};
use tokio::fs;

pub async fn install<R: Runtime>(
    app: &AppHandle<R>,
    cache_dir: Option<String>,
) -> Result<HyMtInstallPaths, String> {
    let manifest = EngineManifest::shipped().map_err(|error| error.to_string())?;
    let runtime = manifest
        .runtime_for_current_arch()
        .map_err(|error| error.to_string())?;
    let cache_root = resolve_cache_root(app, cache_dir)?;
    let paths = HyMtInstallPaths::from_cache_root(cache_root, &manifest, runtime);
    match install_candidate(app, &manifest, runtime, &paths).await {
        Ok(()) => Ok(paths),
        Err(error) => {
            crate::hy_mt_runtime::shutdown_owned();
            if let Some(previous) = recover_active(&paths.root).await {
                emit_progress(app, "Install failed; restored the last known-good engine.");
                Ok(previous)
            } else {
                Err(error)
            }
        }
    }
}

async fn install_candidate<R: Runtime>(
    app: &AppHandle<R>,
    manifest: &EngineManifest,
    runtime: &crate::engine_manifest::RuntimeSpec,
    paths: &HyMtInstallPaths,
) -> Result<(), String> {
    recover_pending_asset(&paths.runtime_dir).await?;
    recover_pending_asset(&paths.model).await?;
    let executable_verified = file_matches(
        &paths.executable,
        runtime.executable.size_bytes,
        &runtime.executable.sha256,
    )
    .await?;
    let model_verified = file_matches(
        &paths.model,
        manifest.model.artifact.size_bytes,
        &manifest.model.artifact.sha256,
    )
    .await?;
    emit_progress(app, "Checking Windows, memory, and storage...");
    crate::engine_preflight::run(
        &paths.root,
        &manifest.requirements,
        !executable_verified || !model_verified,
    )
    .await?;

    fs::create_dir_all(&paths.runtime_dir)
        .await
        .map_err(|error| format!("ENGINE_CREATE_RUNTIME_DIR: {error}"))?;
    fs::create_dir_all(&paths.model_dir)
        .await
        .map_err(|error| format!("ENGINE_CREATE_MODEL_DIR: {error}"))?;
    if let Some(parent) = paths.runtime_archive.parent() {
        fs::create_dir_all(parent)
            .await
            .map_err(|error| format!("ENGINE_CREATE_DOWNLOAD_DIR: {error}"))?;
    }

    let candidate_runtime_dir = candidate_path(&paths.runtime_dir);
    let candidate_executable = candidate_runtime_dir.join(&runtime.executable.relative_path);
    let executable = if executable_verified {
        emit_progress(app, "Translation runtime verified.");
        paths.executable.clone()
    } else {
        let runtime_archive_verified = file_matches(
            &paths.runtime_archive,
            runtime.archive.size_bytes,
            &runtime.archive.sha256,
        )
        .await?;
        if !runtime_archive_verified {
            emit_progress(app, "Downloading the translation runtime...");
            download_file(
                app,
                &runtime.archive.url,
                &paths.runtime_archive,
                Some(runtime.archive.size_bytes),
                "Runtime",
            )
            .await?;
            verify_download(&paths.runtime_archive, &runtime.archive, "RUNTIME").await?;
        }
        emit_progress(app, "Installing or repairing the translation runtime...");
        reset_candidate(&candidate_runtime_dir).await?;
        extract_zip(&paths.runtime_archive, &candidate_runtime_dir).await?;
        if !candidate_executable.is_file() {
            return Err("ENGINE_RUNTIME_INVALID: executable missing after extraction".to_string());
        }
        verify_executable(&candidate_executable, &runtime.executable).await?;
        candidate_executable.clone()
    };

    let candidate_model = candidate_path(&paths.model);
    let model = if !model_verified {
        emit_progress(app, "Downloading Tencent HY-MT (about 1.1 GB)...");
        download_file(
            app,
            &manifest.model.artifact.url,
            &candidate_model,
            Some(manifest.model.artifact.size_bytes),
            "Model",
        )
        .await?;
        verify_download(&candidate_model, &manifest.model.artifact, "MODEL").await?;
        candidate_model.clone()
    } else {
        emit_progress(app, "HY-MT model already downloaded.");
        paths.model.clone()
    };

    let staged = HyMtInstallPaths {
        executable,
        model,
        runtime_dir: if executable_verified {
            paths.runtime_dir.clone()
        } else {
            candidate_runtime_dir.clone()
        },
        model_dir: paths.model_dir.clone(),
        ..paths.clone()
    };
    emit_progress(app, "Warming up and checking a sample translation...");
    verify_sample(&staged, manifest).await?;

    let mut assets = Vec::new();
    if !executable_verified {
        assets.push((candidate_runtime_dir, paths.runtime_dir.clone()));
    }
    if !model_verified {
        assets.push((candidate_model, paths.model.clone()));
    }
    let promotion = promote_assets(&assets).await?;
    let final_verification = async {
        verify_executable(&paths.executable, &runtime.executable).await?;
        verify_download(&paths.model, &manifest.model.artifact, "MODEL").await?;
        let record = InstalledEngine::from_install(paths, manifest, runtime)?;
        record_active(&paths.root, record).await
    }
    .await;
    if let Err(error) = final_verification {
        promotion.rollback().await;
        return Err(error);
    }
    promotion.commit().await?;
    emit_progress(app, "Local Translation Engine verified.");
    Ok(())
}

async fn verify_sample(paths: &HyMtInstallPaths, manifest: &EngineManifest) -> Result<(), String> {
    crate::hy_mt_runtime::shutdown_owned();
    let runtime = paths.managed_config(manifest);
    let result = async {
        let endpoint =
            crate::hy_mt_runtime::ensure_ready(&runtime, Duration::from_secs(90)).await?;
        let config = FoundryLocalConfig {
            model: Some(manifest.model.id.clone()),
            endpoint_url: Some(endpoint),
            managed_runtime: Some(runtime),
            timeout_ms: 90_000,
            ..FoundryLocalConfig::default()
        };
        FoundryLocalBackend::new(config)
            .translate("先不提时钟塔", "zh-CN", "en-US")
            .await
            .map_err(|error| format!("ENGINE_SAMPLE_TRANSLATION_FAILED: {error}"))
    }
    .await;
    crate::hy_mt_runtime::shutdown_owned();
    let translated = result?;
    if translated.trim().is_empty() || translated.trim() == "先不提时钟塔" {
        return Err("ENGINE_SAMPLE_TRANSLATION_FAILED".to_string());
    }
    Ok(())
}

fn candidate_path(path: &Path) -> PathBuf {
    let mut candidate = path.as_os_str().to_os_string();
    candidate.push(".candidate");
    PathBuf::from(candidate)
}

fn resolve_cache_root<R: Runtime>(
    app: &AppHandle<R>,
    cache_dir: Option<String>,
) -> Result<PathBuf, String> {
    let root = match cache_dir.as_deref().map(str::trim) {
        Some(path) if !path.is_empty() && !path.contains('%') => PathBuf::from(path),
        _ => app
            .path()
            .app_cache_dir()
            .map_err(|error| format!("ENGINE_CACHE_PATH: {error}"))?,
    };
    if !root.is_absolute() {
        return Err("ENGINE_CACHE_PATH: storage directory must be absolute".to_string());
    }
    Ok(root)
}

fn emit_progress<R: Runtime>(app: &AppHandle<R>, line: impl Into<String>) {
    let _ = app.emit_to(
        "foundry-wizard",
        "wizard-output",
        serde_json::json!({"stream": "stdout", "line": line.into()}),
    );
}
