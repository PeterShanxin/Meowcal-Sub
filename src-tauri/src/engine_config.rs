use crate::config::FoundryLocalConfig;
pub use meowcal_core::config::ManagedLocalRuntimeConfig;
use std::path::{Path, PathBuf};

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
        if self.engine_cache_root.is_none() {
            self.engine_cache_root = current.engine_cache_root.clone();
        }
    }

    pub fn managed_cache_root(&self) -> Option<PathBuf> {
        let runtime = self.managed_runtime.as_ref()?;
        let executable_root = cache_root(Path::new(&runtime.executable_path))?;
        let model_root = cache_root(Path::new(&runtime.model_path))?;
        (executable_root == model_root).then_some(executable_root)
    }
}

fn cache_root(path: &Path) -> Option<PathBuf> {
    if let Some(root) = path.ancestors().find(|ancestor| {
        ancestor
            .file_name()
            .is_some_and(|name| name == "meowcal-sub")
    }) {
        return root.parent().map(Path::to_path_buf);
    }
    path.ancestors()
        .find_map(|architecture| core_storage_base(architecture).map(Path::to_path_buf))
}

pub(crate) fn core_storage_base(root: &Path) -> Option<&Path> {
    let architecture = root.file_name()?.to_str()?;
    if architecture != "aarch64" && architecture != "x86_64" {
        return None;
    }
    let version = root.parent()?.file_name()?.to_str()?;
    if version.split('.').count() != 3
        || version
            .split('.')
            .any(|part| part.is_empty() || !part.bytes().all(|byte| byte.is_ascii_digit()))
    {
        return None;
    }
    let profile = root.parent()?.parent()?;
    if !matches!(profile.file_name()?.to_str()?, "production" | "development") {
        return None;
    }
    profile.parent()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn derives_legacy_and_core_storage_bases() {
        let mut config = FoundryLocalConfig::default();
        config.managed_runtime = Some(ManagedLocalRuntimeConfig {
            kind: "hy-mt".to_string(),
            executable_path: r"D:\cache\meowcal-sub\runtime\server.exe".to_string(),
            model_path: r"D:\cache\meowcal-sub\models\model.gguf".to_string(),
            port: 11_436,
        });
        assert_eq!(
            config.managed_cache_root(),
            Some(PathBuf::from(r"D:\cache"))
        );

        let runtime = config.managed_runtime.as_mut().unwrap();
        runtime.executable_path =
            r"E:\shared\production\0.1.0\x86_64\runtime\server.exe".to_string();
        runtime.model_path = r"E:\shared\production\0.1.0\x86_64\models\model.gguf".to_string();
        assert_eq!(
            config.managed_cache_root(),
            Some(PathBuf::from(r"E:\shared"))
        );

        let runtime = config.managed_runtime.as_mut().unwrap();
        runtime.executable_path =
            r"F:\shared\development\0.2.3\aarch64\runtime\server.exe".to_string();
        runtime.model_path = r"F:\shared\development\0.2.3\aarch64\models\model.gguf".to_string();
        assert_eq!(
            config.managed_cache_root(),
            Some(PathBuf::from(r"F:\shared"))
        );
    }

    #[test]
    fn rejects_non_versioned_or_unknown_core_partitions() {
        assert!(core_storage_base(Path::new(r"D:\shared\production\current\x86_64")).is_none());
        assert!(core_storage_base(Path::new(r"D:\shared\preview\0.1.0\x86_64")).is_none());
    }
}
