use crate::hy_mt_runtime::HyMtInstallPaths;
use std::sync::Arc;
use tauri::{AppHandle, Emitter, Manager, Runtime};

/// Thin Tauri adapter over the shared Core installer. Storage selection and
/// artifact verification belong to Core; this module only forwards progress.
pub async fn install<R: Runtime>(
    app: &AppHandle<R>,
    cache_dir: Option<String>,
) -> Result<HyMtInstallPaths, String> {
    if let Some(path) = cache_dir
        .as_deref()
        .map(str::trim)
        .filter(|path| !path.is_empty())
    {
        let selected = std::path::PathBuf::from(path);
        let default = app
            .path()
            .app_cache_dir()
            .map_err(|error| format!("ENGINE_CACHE_PATH: {error}"))?;
        crate::core_client::select_storage_root((selected != default).then_some(selected))?;
    }
    let progress_app = app.clone();
    let progress = Arc::new(move |line: String| emit_progress(&progress_app, line));
    let status = crate::core_client::install(progress).await?;
    if !status.installed {
        return Err("ENGINE_INSTALL_INCOMPLETE".to_string());
    }
    let paths = status
        .install_paths
        .ok_or_else(|| "CORE_INSTALL_PATHS_MISSING".to_string())?;
    Ok(HyMtInstallPaths {
        root: paths.root,
        runtime_dir: paths.runtime_dir,
        runtime_archive: paths.runtime_archive,
        executable: paths.executable,
        model_dir: paths.model_dir,
        model: paths.model,
    })
}

fn emit_progress<R: Runtime>(app: &AppHandle<R>, line: impl Into<String>) {
    let _ = app.emit_to(
        "foundry-wizard",
        "wizard-output",
        serde_json::json!({"stream": "stdout", "line": line.into()}),
    );
}
