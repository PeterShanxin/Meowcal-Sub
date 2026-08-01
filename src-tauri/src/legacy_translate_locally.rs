//! Compatibility-only translateLocally commands.
//!
//! The curated product path uses the app-managed Tencent HY-MT engine. These
//! commands remain registered for older profiles and developer diagnostics,
//! but they are intentionally kept outside the normal setup and translation
//! adapters.

use reqwest::Client;
use serde::Serialize;
use std::path::PathBuf;
use tauri::{AppHandle, Manager};
use tauri_plugin_opener::OpenerExt;
use tokio::fs;

const TRANSLATE_LOCALLY_BASE_URL: &str =
    "https://github.com/XapaJIaMnu/translateLocally/releases/download/latest";

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TranslateLocallyDownloadOption {
    pub id: String,
    pub label: String,
    pub asset_name: String,
    pub url: String,
    pub notes: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TranslateLocallyDownloadInfo {
    pub recommended_id: String,
    pub default_install_dir: String,
    pub options: Vec<TranslateLocallyDownloadOption>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TranslateLocallyDownloadResult {
    pub path: String,
    pub option_id: String,
    pub used_fallback: bool,
    pub notes: String,
}

/// Open the compatibility translateLocally download page in the default browser.
#[tauri::command]
pub fn open_translate_locally_download(app: AppHandle) -> Result<(), String> {
    let url = "https://github.com/XapaJIaMnu/translateLocally/releases/tag/latest";
    app.opener()
        .open_url(url, None::<&str>)
        .map_err(|e| e.to_string())
}

/// Get compatibility translateLocally download options for this machine.
#[tauri::command]
pub fn get_translate_locally_download_info(
    app: AppHandle,
) -> Result<TranslateLocallyDownloadInfo, String> {
    build_download_info(&app)
}

/// Download the compatibility translateLocally binary.
#[tauri::command]
pub async fn download_translate_locally(
    app: AppHandle,
    option_id: Option<String>,
    install_dir: String,
) -> Result<TranslateLocallyDownloadResult, String> {
    let download_info = build_download_info(&app)?;
    let mut options = download_info.options;

    if options.is_empty() {
        return Err("No translateLocally builds available for this platform.".to_string());
    }

    let requested = option_id
        .and_then(|id| {
            let trimmed = id.trim().to_string();
            (!trimmed.is_empty()).then_some(trimmed)
        })
        .unwrap_or_else(|| download_info.recommended_id.clone());

    let mut order = Vec::new();
    if let Some(index) = options.iter().position(|option| option.id == requested) {
        order.push(options.remove(index));
    }
    order.extend(options);

    let target_path = resolve_install_target(&app, &install_dir)?;
    if let Some(parent) = target_path.parent() {
        fs::create_dir_all(parent)
            .await
            .map_err(|e| format!("Failed to create install dir: {}", e))?;
    }

    let mut last_error = None;
    for (index, option) in order.iter().enumerate() {
        match download_asset(&option.url, &target_path).await {
            Ok(()) => {
                let used_fallback = index > 0;
                let notes = if used_fallback {
                    format!("Downloaded fallback build: {}", option.label)
                } else {
                    format!("Downloaded: {}", option.label)
                };
                return Ok(TranslateLocallyDownloadResult {
                    path: target_path.to_string_lossy().to_string(),
                    option_id: option.id.clone(),
                    used_fallback,
                    notes,
                });
            }
            Err(error) => last_error = Some(error),
        }
    }

    Err(last_error.unwrap_or_else(|| "Download failed.".to_string()))
}

fn build_download_info(app: &AppHandle) -> Result<TranslateLocallyDownloadInfo, String> {
    let options = download_options()?;
    if options.is_empty() {
        return Err("No translateLocally builds found for this platform.".to_string());
    }

    let recommended_id = options
        .first()
        .map(|option| option.id.clone())
        .unwrap_or_else(|| "unknown".to_string());
    Ok(TranslateLocallyDownloadInfo {
        recommended_id,
        default_install_dir: default_install_dir(app).to_string_lossy().to_string(),
        options,
    })
}

fn download_options() -> Result<Vec<TranslateLocallyDownloadOption>, String> {
    if std::env::consts::OS != "windows" {
        return Err("In-app download is only available on Windows.".to_string());
    }

    let mut options = Vec::new();
    match std::env::consts::ARCH {
        "aarch64" => options.push(build_option(
            "win-x64",
            "Windows x64 (non-AVX) - recommended for ARM64",
            "translateLocally.windows-2019.x86-64.exe",
            "Runs under x64 emulation. AVX builds will not run on ARM64.",
        )),
        "x86_64" => {
            if supports_avx() {
                options.push(build_option(
                    "win-avx",
                    "Windows x64 (AVX optimized)",
                    "translateLocally.windows-2022.core-avx-i.exe",
                    "Fastest option if your CPU supports AVX.",
                ));
            }
            options.push(build_option(
                "win-x64",
                "Windows x64 (non-AVX)",
                "translateLocally.windows-2019.x86-64.exe",
                "Most compatible option for older CPUs.",
            ));
        }
        architecture => return Err(format!("Unsupported CPU architecture: {}", architecture)),
    }
    Ok(options)
}

fn build_option(
    id: &str,
    label: &str,
    asset_name: &str,
    notes: &str,
) -> TranslateLocallyDownloadOption {
    TranslateLocallyDownloadOption {
        id: id.to_string(),
        label: label.to_string(),
        asset_name: asset_name.to_string(),
        url: format!("{}/{}", TRANSLATE_LOCALLY_BASE_URL, asset_name),
        notes: notes.to_string(),
    }
}

fn supports_avx() -> bool {
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    {
        std::is_x86_feature_detected!("avx")
    }
    #[cfg(not(any(target_arch = "x86", target_arch = "x86_64")))]
    {
        false
    }
}

fn default_install_dir(app: &AppHandle) -> PathBuf {
    if cfg!(target_os = "windows") {
        if let Ok(local_appdata) = std::env::var("LOCALAPPDATA") {
            return PathBuf::from(local_appdata).join("translateLocally");
        }
    }
    app.path()
        .app_data_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
        .join("translateLocally")
}

fn resolve_install_target(app: &AppHandle, raw_input: &str) -> Result<PathBuf, String> {
    let trimmed = raw_input.trim();
    if trimmed.is_empty() {
        return Err("Install path is required.".to_string());
    }

    let path = PathBuf::from(trimmed);
    let mut resolved = if path.is_absolute() {
        path
    } else {
        app.path()
            .app_data_dir()
            .map_err(|e| format!("Failed to resolve app data dir: {}", e))?
            .join(path)
    };
    if resolved.extension().is_none() {
        resolved.push(if cfg!(target_os = "windows") {
            "translateLocally.exe"
        } else {
            "translateLocally"
        });
    }
    Ok(resolved)
}

async fn download_asset(url: &str, target_path: &PathBuf) -> Result<(), String> {
    let client = Client::builder()
        .user_agent("Meowcal-Sub/0.1.0")
        .build()
        .map_err(|e| format!("Failed to build HTTP client: {}", e))?;
    let response = client
        .get(url)
        .send()
        .await
        .map_err(|e| format!("Download request failed: {}", e))?;
    if !response.status().is_success() {
        return Err(format!("Download failed: HTTP {}", response.status()));
    }
    let bytes = response
        .bytes()
        .await
        .map_err(|e| format!("Failed to read download: {}", e))?;
    fs::write(target_path, &bytes)
        .await
        .map_err(|e| format!("Failed to write file: {}", e))
}
