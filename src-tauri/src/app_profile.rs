/// Application data identities for direct and Microsoft Store distributions.
///
/// Normal debug development builds use the development namespace. Release
/// builds retain the installed application's production namespace.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppProfile {
    Production,
    Development,
    Store,
    StoreDevelopment,
}

impl AppProfile {
    /// Resolve the profile for the build running this process.
    pub const fn current() -> Self {
        if cfg!(feature = "store") {
            if cfg!(debug_assertions) {
                Self::StoreDevelopment
            } else {
                Self::Store
            }
        } else if cfg!(debug_assertions) {
            Self::Development
        } else {
            Self::Production
        }
    }

    /// The namespace used by Tauri's platform path resolver.
    pub const fn identifier(self) -> &'static str {
        match self {
            Self::Production => "com.meowcal.sub",
            Self::Development => "com.meowcal.sub.dev",
            Self::Store => "com.meowcal.sub.store",
            Self::StoreDevelopment => "com.meowcal.sub.store.dev",
        }
    }

    /// The compact name used on existing app surfaces.
    pub const fn display_name(self) -> &'static str {
        match self {
            Self::Production | Self::Store => "Meowcal Sub",
            Self::Development | Self::StoreDevelopment => "Meowcal Sub - Dev",
        }
    }
}

/// Downloaded inference processes do not inherit package identity. Give Core
/// the physical package cache path, not an AppData path redirected only for
/// the packaged parent, so the child can load its libraries and model.
pub fn store_core_storage_base() -> Result<Option<std::path::PathBuf>, String> {
    #[cfg(all(target_os = "windows", feature = "store"))]
    {
        let cache = windows::Storage::ApplicationData::Current()
            .and_then(|data| data.LocalCacheFolder())
            .and_then(|folder| folder.Path())
            .map_err(|error| {
                format!("CORE_STORE_CACHE: launch through the registered MSIX package: {error}")
            })?;
        Ok(Some(
            std::path::PathBuf::from(cache.to_string())
                .join(AppProfile::current().identifier())
                .join("Core"),
        ))
    }
    #[cfg(not(all(target_os = "windows", feature = "store")))]
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn profiles_have_distinct_namespaces() {
        assert_eq!(AppProfile::Production.identifier(), "com.meowcal.sub");
        assert_eq!(AppProfile::Development.identifier(), "com.meowcal.sub.dev");
        assert_eq!(AppProfile::Store.identifier(), "com.meowcal.sub.store");
        assert_eq!(
            AppProfile::StoreDevelopment.identifier(),
            "com.meowcal.sub.store.dev"
        );
    }

    #[test]
    fn only_development_has_the_visible_suffix() {
        assert_eq!(AppProfile::Production.display_name(), "Meowcal Sub");
        assert_eq!(AppProfile::Development.display_name(), "Meowcal Sub - Dev");
    }

    #[test]
    fn current_profile_matches_the_build_kind() {
        let expected = if cfg!(feature = "store") {
            if cfg!(debug_assertions) {
                AppProfile::StoreDevelopment
            } else {
                AppProfile::Store
            }
        } else if cfg!(debug_assertions) {
            AppProfile::Development
        } else {
            AppProfile::Production
        };
        assert_eq!(AppProfile::current(), expected);
    }
}
