use crate::engine_artifact_io::{
    download_file, extract_zip, file_matches, verify_download, verify_executable,
};
use crate::engine_install_transaction::{
    promote_assets, record_active, recover_active, recover_pending_asset, reset_candidate,
    InstalledEngine,
};
use crate::engine_manifest::EngineManifest;
use crate::hy_mt_runtime::HyMtInstallPaths;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tokio::fs;

pub async fn install(
    progress: &(dyn Fn(String) + Send + Sync),
    root: PathBuf,
) -> Result<HyMtInstallPaths, String> {
    install_with_download_policy(progress, root, true).await
}

pub async fn install_local(
    progress: &(dyn Fn(String) + Send + Sync),
    root: PathBuf,
) -> Result<HyMtInstallPaths, String> {
    install_with_download_policy(progress, root, false).await
}

async fn install_with_download_policy(
    progress: &(dyn Fn(String) + Send + Sync),
    root: PathBuf,
    allow_download: bool,
) -> Result<HyMtInstallPaths, String> {
    let manifest = EngineManifest::shipped().map_err(|error| error.to_string())?;
    let runtime = manifest
        .runtime_for_current_arch()
        .map_err(|error| error.to_string())?;
    let paths = HyMtInstallPaths::from_cache_root(root, &manifest, runtime);
    match install_candidate(progress, &manifest, runtime, &paths, allow_download).await {
        Ok(()) => Ok(paths),
        Err(error) => {
            crate::hy_mt_runtime::shutdown_owned();
            if let Some(previous) = recover_active(&paths.root).await {
                crate::storage::verify(&previous, &manifest).await?;
                // Recovery succeeds, but the caller still needs the repair failure.
                tracing::error!("Engine install failed, restored the previous one: {error}");
                emit_progress(
                    progress,
                    format!("Install failed ({error}); restored the last known-good engine."),
                );
                Ok(previous)
            } else {
                Err(error)
            }
        }
    }
}

async fn install_candidate(
    progress: &(dyn Fn(String) + Send + Sync),
    manifest: &EngineManifest,
    runtime: &crate::engine_manifest::RuntimeSpec,
    paths: &HyMtInstallPaths,
    allow_download: bool,
) -> Result<(), String> {
    recover_pending_asset(&paths.runtime_dir).await?;
    recover_pending_asset(&paths.model).await?;
    let executable_verified = crate::storage::verify_runtime(paths, manifest)
        .await
        .is_ok();
    let model_verified = file_matches(
        &paths.model,
        manifest.model.artifact.size_bytes,
        &manifest.model.artifact.sha256,
    )
    .await?;
    emit_progress(progress, "Checking Windows, memory, and storage...");
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
        emit_progress(progress, "Translation runtime verified.");
        paths.executable.clone()
    } else {
        let runtime_archive_verified = file_matches(
            &paths.runtime_archive,
            runtime.archive.size_bytes,
            &runtime.archive.sha256,
        )
        .await?;
        if !runtime_archive_verified {
            if !allow_download {
                return Err("CORE_ASSETS_UNVERIFIED: runtime archive is unavailable; install or repair is required".into());
            }
            emit_progress(progress, "Downloading the translation runtime...");
            download_file(
                progress,
                &runtime.archive.url,
                &paths.runtime_archive,
                Some(runtime.archive.size_bytes),
                "Runtime",
            )
            .await?;
            verify_download(&paths.runtime_archive, &runtime.archive, "RUNTIME").await?;
        }
        emit_progress(
            progress,
            "Installing or repairing the translation runtime...",
        );
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
        if !allow_download {
            return Err(
                "CORE_ASSETS_UNVERIFIED: model is unavailable; install or repair is required"
                    .into(),
            );
        }
        emit_progress(progress, "Downloading Tencent HY-MT (about 1.1 GB)...");
        download_file(
            progress,
            &manifest.model.artifact.url,
            &candidate_model,
            Some(manifest.model.artifact.size_bytes),
            "Model",
        )
        .await?;
        verify_download(&candidate_model, &manifest.model.artifact, "MODEL").await?;
        candidate_model.clone()
    } else {
        emit_progress(progress, "HY-MT model already downloaded.");
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
    emit_progress(progress, "Warming up and checking a sample translation...");
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
        // The unchanged model was verified before the sample under the same
        // exclusive lease. Only newly promoted model bytes need this check.
        if !model_verified {
            verify_download(&paths.model, &manifest.model.artifact, "MODEL").await?;
        }
        let record = InstalledEngine::from_install(paths, manifest, runtime)?;
        record_active(&paths.root, record).await
    }
    .await;
    if let Err(error) = final_verification {
        promotion.rollback().await;
        return Err(error);
    }
    promotion.commit().await?;
    emit_progress(progress, "Local Translation Engine verified.");
    Ok(())
}

async fn verify_sample(paths: &HyMtInstallPaths, manifest: &EngineManifest) -> Result<(), String> {
    crate::hy_mt_runtime::shutdown_owned();
    let runtime = paths.managed_config(manifest);
    let result = async {
        let endpoint =
            crate::hy_mt_runtime::ensure_ready(&runtime, Duration::from_secs(90)).await?;
        crate::completion::sample(&endpoint, &manifest.model.id).await
    }
    .await;
    crate::hy_mt_runtime::shutdown_owned();
    result
}
fn candidate_path(path: &Path) -> PathBuf {
    let mut candidate = path.as_os_str().to_os_string();
    candidate.push(".candidate");
    PathBuf::from(candidate)
}

fn emit_progress(progress: &(dyn Fn(String) + Send + Sync), line: impl Into<String>) {
    progress(line.into());
}
