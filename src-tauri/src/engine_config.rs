use crate::config::FoundryLocalConfig;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ManagedLocalRuntimeConfig {
    pub kind: String,
    pub executable_path: String,
    pub model_path: String,
    pub port: u16,
}

impl FoundryLocalConfig {
    pub fn effective_endpoint_url(&self) -> Option<String> {
        if let Some(runtime) = self.managed_runtime.as_ref() {
            return Some(crate::hy_mt_runtime::endpoint_url(runtime));
        }
        self.endpoint_url
            .as_deref()
            .map(str::trim)
            .filter(|url| !url.is_empty())
            .map(|url| url.trim_end_matches('/').to_string())
    }

    pub fn is_translation_only_model(&self) -> bool {
        self.managed_runtime
            .as_ref()
            .is_some_and(|runtime| runtime.kind.eq_ignore_ascii_case("hy-mt"))
    }

    pub fn preserve_managed_runtime_from(&mut self, current: &Self) {
        if current.managed_runtime.is_some() {
            self.model = current.model.clone();
            self.endpoint_url = current.endpoint_url.clone();
            self.managed_runtime = current.managed_runtime.clone();
        }
        // Carried across independently of the runtime record: knowing where the
        // engine lives is what lets a lost registration be recovered rather than
        // re-downloaded (#65), so it must outlive the record itself.
        if self.engine_cache_root.is_none() {
            self.engine_cache_root = current.engine_cache_root.clone();
        }
    }

    pub fn managed_cache_root(&self) -> Option<PathBuf> {
        let runtime = self.managed_runtime.as_ref()?;
        let executable_root = engine_root(Path::new(&runtime.executable_path))?;
        let model_root = engine_root(Path::new(&runtime.model_path))?;
        (executable_root == model_root)
            .then(|| executable_root.parent().map(Path::to_path_buf))
            .flatten()
    }
}

fn engine_root(path: &Path) -> Option<&Path> {
    path.ancestors().find(|ancestor| {
        ancestor
            .file_name()
            .is_some_and(|name| name == "meowcal-sub")
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::TranslationConfig;
    use crate::llm::FoundryLocalBackend;

    fn managed_config() -> FoundryLocalConfig {
        FoundryLocalConfig {
            model: Some("HY-MT1.5-1.8B-Q4_K_M".to_string()),
            endpoint_url: Some("http://127.0.0.1:11436".to_string()),
            managed_runtime: Some(ManagedLocalRuntimeConfig {
                kind: "hy-mt".to_string(),
                executable_path: r"D:\engine\llama-server.exe".to_string(),
                model_path: r"D:\engine\HY-MT.gguf".to_string(),
                port: 11_436,
            }),
            ..FoundryLocalConfig::default()
        }
    }

    #[test]
    fn managed_hy_mt_round_trips_and_disables_passthrough() {
        let mut config = TranslationConfig {
            allow_mock_fallback: true,
            foundry_local: managed_config(),
            ..TranslationConfig::default()
        };
        config.normalize();
        let json = serde_json::to_string(&config).unwrap();
        let restored: TranslationConfig = serde_json::from_str(&json).unwrap();

        assert!(!config.allow_mock_fallback);
        assert!(restored.foundry_local.is_translation_only_model());
        assert_eq!(
            restored.foundry_local.endpoint_url.as_deref(),
            Some("http://127.0.0.1:11436")
        );
    }

    #[test]
    fn settings_save_preserves_app_owned_paths() {
        let current = managed_config();
        let mut incoming = FoundryLocalConfig::default();
        incoming.preserve_managed_runtime_from(&current);

        assert_eq!(incoming.model, current.model);
        assert_eq!(incoming.endpoint_url, current.endpoint_url);
        assert!(incoming.managed_runtime.is_some());
    }

    #[test]
    fn managed_endpoint_selects_hy_mt_without_catalog() {
        let backend = FoundryLocalBackend::new(managed_config());
        assert_eq!(
            backend.selected_model().as_deref(),
            Some("HY-MT1.5-1.8B-Q4_K_M")
        );
    }

    #[test]
    fn repair_reuses_the_app_managed_cache_root() {
        let mut config = managed_config();
        let runtime = config.managed_runtime.as_mut().unwrap();
        runtime.executable_path =
            r"D:\foundry-cache\meowcal-sub\runtime\engine\server.exe".to_string();
        runtime.model_path = r"D:\foundry-cache\meowcal-sub\models\model\model.gguf".to_string();
        assert_eq!(
            config.managed_cache_root(),
            Some(PathBuf::from(r"D:\foundry-cache"))
        );
    }

    #[test]
    fn mismatched_managed_paths_are_not_adopted() {
        let mut config = managed_config();
        let runtime = config.managed_runtime.as_mut().unwrap();
        runtime.executable_path = r"D:\one\meowcal-sub\runtime\server.exe".to_string();
        runtime.model_path = r"E:\two\meowcal-sub\models\model.gguf".to_string();
        assert_eq!(config.managed_cache_root(), None);
    }
}
