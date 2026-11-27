//! Portable builds only offer the official ZIP for the running architecture.
use tauri_plugin_opener::OpenerExt;

const REPO_URL: &str = "https://github.com/PeterShanxin/Meowcal-Sub";
const RELEASE_API: &str = "https://api.github.com/repos/PeterShanxin/Meowcal-Sub/releases/latest";

#[derive(serde::Deserialize)]
struct Release {
    tag_name: String,
    draft: bool,
    prerelease: bool,
    assets: Vec<Asset>,
}

#[derive(serde::Deserialize)]
struct Asset {
    name: String,
    state: String,
    browser_download_url: String,
}

fn zip_for(version: &str, architecture: &str) -> Option<(String, String)> {
    let is_canonical = version.split('.').count() == 3
        && version.split('.').all(|part| {
            part.parse::<u64>()
                .ok()
                .is_some_and(|number| number.to_string() == part)
        });
    if !is_canonical || !matches!(architecture, "x64" | "arm64") {
        return None;
    }
    let name = format!("Meowcal.Sub_{version}_{architecture}-portable.zip");
    let url = format!("{REPO_URL}/releases/download/v{version}/{name}");
    Some((name, url))
}

fn advertised_version(
    release: &Release,
    current: &tauri::PackageInfo,
    arch: &str,
) -> Option<String> {
    if release.draft || release.prerelease {
        return None;
    }
    let version = release.tag_name.strip_prefix('v')?;
    let (name, url) = zip_for(version, arch)?;
    if version
        .parse()
        .ok()
        .is_some_and(|parsed| current.version >= parsed)
    {
        return None;
    }
    release
        .assets
        .iter()
        .any(|asset| {
            asset.state == "uploaded" && asset.name == name && asset.browser_download_url == url
        })
        .then(|| version.to_owned())
}

fn architecture() -> &'static str {
    match std::env::consts::ARCH {
        "aarch64" => "arm64",
        "x86_64" => "x64",
        _ => "",
    }
}

#[tauri::command]
pub async fn check_portable_update(app: tauri::AppHandle) -> Result<Option<String>, String> {
    let error_message = "Could not check for portable updates. Try again.";
    if !cfg!(feature = "portable") {
        return Err(error_message.into());
    }
    tracing::info!("Checking the official portable ZIP channel");
    let check = async {
        let response = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(std::time::Duration::from_secs(8))
            .build()?
            .get(RELEASE_API)
            .header("User-Agent", "Meowcal-Sub-Portable")
            .send()
            .await?;
        if response.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(None);
        }
        let release = response.error_for_status()?.json::<Release>().await?;
        Ok::<_, reqwest::Error>(advertised_version(
            &release,
            app.package_info(),
            architecture(),
        ))
    };
    check.await.map_err(|error| {
        tracing::warn!(%error, "Could not query the official portable release");
        error_message.into()
    })
}

#[tauri::command]
pub fn open_portable_update(app: tauri::AppHandle, version: String) -> Result<(), String> {
    let error_message = "Could not open the portable download. Try again.";
    if !cfg!(feature = "portable") {
        return Err(error_message.into());
    }
    let (_, url) = zip_for(&version, architecture()).ok_or(error_message)?;
    app.opener().open_url(url, None::<&str>).map_err(|error| {
        tracing::warn!(%error, "Could not open the portable ZIP download");
        error_message.into()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_same_architecture_and_official_url_required() {
        let current = tauri::PackageInfo {
            version: "0.11.0".parse().unwrap(),
            name: String::new(),
            authors: "",
            description: "",
            crate_name: "",
        };
        let (name, browser_download_url) = zip_for("0.12.0", "arm64").unwrap();
        let mut release = Release {
            tag_name: "v0.12.0".into(),
            draft: false,
            prerelease: false,
            assets: vec![Asset {
                name,
                state: "uploaded".into(),
                browser_download_url,
            }],
        };
        assert_eq!(
            advertised_version(&release, &current, "arm64").as_deref(),
            Some("0.12.0")
        );
        assert_eq!(advertised_version(&release, &current, "x64"), None);
        release.assets[0].browser_download_url = "https://example.com/foreign.zip".into();
        assert_eq!(advertised_version(&release, &current, "arm64"), None);
    }

    #[test]
    fn zip_does_not_allow_prerelease_or_path_elements() {
        assert!(zip_for("0.12.0-pre", "arm64").is_none());
        assert!(zip_for("../0.12.0", "arm64").is_none());
        assert!(zip_for("0.12.0", "arm").is_none());
    }
}
