//! Enumerating and installing the Windows OCR language packs.
//!
//! Installing one needs elevation, so the tag reaches an elevated PowerShell
//! command line. It is mapped through a fixed allowlist rather than escaped:
//! an unknown tag is refused, never quoted and passed on.

use tauri::async_runtime;
use tracing::{info, warn};

use crate::ocr::CoreOcr;
use crate::ui_language::Locale;

/// List OCR language packs installed on this system.
/// Returns BCP-47 tags (e.g. ["en-US", "zh-CN"]).
pub async fn available() -> Vec<String> {
    info!("Getting available OCR languages...");
    let result = async_runtime::spawn_blocking(CoreOcr::available_languages).await;
    match result {
        Ok(Ok(langs)) => {
            info!("Found {} OCR language(s): {:?}", langs.len(), langs);
            langs
        }
        Ok(Err(e)) => {
            warn!("Failed to enumerate OCR languages: {}", e);
            Vec::new()
        }
        Err(e) => {
            warn!("OCR language enumeration task failed: {}", e);
            Vec::new()
        }
    }
}

/// Map a BCP-47 tag to the Windows capability tag used by `Get-WindowsCapability`.
///
/// Strict allowlist: only accept known BCP-47 tags to prevent command injection
/// in the elevated PowerShell context.
pub fn capability_tag(language_tag: &str) -> Result<&'static str, String> {
    match language_tag {
        "en-US" => Ok("en-US"),
        "zh-TW" => Ok("zh-Hant"),
        "zh-CN" => Ok("zh-Hans"),
        "ja-JP" => Ok("ja"),
        "ko-KR" => Ok("ko"),
        "es-ES" => Ok("es"),
        "fr-FR" => Ok("fr"),
        "de-DE" => Ok("de"),
        _ => Err(format!(
            "Unsupported language tag: '{}'. Only known languages can be installed.",
            language_tag
        )),
    }
}

/// Install an OCR language pack via an elevated PowerShell window.
/// Triggers a UAC prompt — the user must approve the elevation.
pub async fn install(language_tag: String, locale: Locale) -> Result<(), String> {
    let capability_tag = capability_tag(&language_tag)?.to_string();

    info!(
        "Installing OCR language pack: {} (capability tag: {})",
        language_tag, capability_tag
    );

    async_runtime::spawn_blocking(move || {
        // Build the inner (elevated) PowerShell script
        let inner_script = installation_script(&capability_tag, locale);

        // Outer PowerShell spawns an elevated inner shell via Start-Process -Verb RunAs
        let mut cmd = crate::windowless_command::std_command("powershell");
        cmd.args([
            "-NoProfile",
            "-Command",
            &format!(
                "Start-Process powershell -Verb RunAs -Wait -ArgumentList '-NoProfile -Command {}'",
                // Escape single quotes for the nested argument
                inner_script.replace('\'', "''")
            ),
        ]);

        match cmd.status() {
            Ok(status) if status.success() => {
                info!("OCR language pack install completed for: {}", language_tag);
                Ok(())
            }
            Ok(status) => {
                let msg = format!(
                    "OCR language pack install exited with code: {:?}",
                    status.code()
                );
                warn!("{}", msg);
                // Still return Ok — the user may have cancelled the UAC prompt,
                // and we'll re-check available languages on the frontend
                Ok(())
            }
            Err(e) => Err(format!("Failed to launch installer: {}", e)),
        }
    })
    .await
    .map_err(|e| e.to_string())?
}

fn installation_script(capability_tag: &str, locale: Locale) -> String {
    let installing = locale.text("Installing OCR language pack", "正在安装 OCR 语言包");
    let complete = locale.text(
        "OCR language pack installed successfully.",
        "OCR 语言包安装成功。",
    );
    let unavailable = locale.text(
        "Language pack is already installed or not available.",
        "语言包已安装或暂不可用。",
    );
    format!(
        "Write-Host '{installing}: {capability_tag}...' -ForegroundColor Cyan; \
         Write-Host ''; \
         $cap = Get-WindowsCapability -Online | Where-Object {{ $_.Name -Like 'Language.OCR*{capability_tag}*' -and $_.State -ne 'Installed' }}; \
         if ($cap) {{ \
             $cap | Add-WindowsCapability -Online; \
             Write-Host ''; \
             Write-Host '{complete}' -ForegroundColor Green \
         }} else {{ \
             Write-Host '{unavailable}' -ForegroundColor Yellow \
         }}; \
         Start-Sleep -Seconds 5"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn installation_copy_follows_ui_language_without_translating_capability_names() {
        let english = installation_script("zh-Hans", Locale::English);
        let chinese = installation_script("zh-Hans", Locale::SimplifiedChinese);
        assert!(english.contains("Installing OCR language pack"));
        assert!(chinese.contains("正在安装 OCR 语言包"));
        assert!(chinese.contains("OCR 语言包安装成功。"));
        for script in [english, chinese] {
            assert!(script.contains("Language.OCR*zh-Hans*"));
            assert!(script.contains("Add-WindowsCapability -Online"));
        }
    }

    #[test]
    fn maps_every_offered_language_to_its_capability_tag() {
        assert_eq!(capability_tag("en-US"), Ok("en-US"));
        assert_eq!(capability_tag("zh-TW"), Ok("zh-Hant"));
        assert_eq!(capability_tag("zh-CN"), Ok("zh-Hans"));
        assert_eq!(capability_tag("ja-JP"), Ok("ja"));
        assert_eq!(capability_tag("ko-KR"), Ok("ko"));
        assert_eq!(capability_tag("es-ES"), Ok("es"));
        assert_eq!(capability_tag("fr-FR"), Ok("fr"));
        assert_eq!(capability_tag("de-DE"), Ok("de"));
    }

    #[test]
    fn refuses_a_tag_that_is_not_on_the_allowlist() {
        assert_eq!(
            capability_tag("en-GB"),
            Err(
                "Unsupported language tag: 'en-GB'. Only known languages can be installed."
                    .to_string()
            )
        );
    }

    /// The tag is interpolated into an elevated command line, so anything that
    /// could close a quote or chain a statement has to be refused outright
    /// rather than escaped.
    #[test]
    fn refuses_injection_shaped_tags_instead_of_escaping_them() {
        for hostile in [
            "en-US'; Start-Process calc; '",
            "en-US*",
            "'",
            "",
            "en-us",
            " en-US ",
        ] {
            assert!(
                capability_tag(hostile).is_err(),
                "expected {hostile:?} to be refused"
            );
        }
    }
}
