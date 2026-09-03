/// The two application identities owned by this repository.
///
/// Normal debug development builds use the development namespace. Release
/// builds retain the installed application's production namespace.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppProfile {
    Production,
    Development,
}

impl AppProfile {
    /// Resolve the profile for the build running this process.
    pub const fn current() -> Self {
        if cfg!(debug_assertions) {
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
        }
    }

    /// The compact name used on existing app surfaces.
    pub const fn display_name(self) -> &'static str {
        match self {
            Self::Production => "Meowcal Sub",
            Self::Development => "Meowcal Sub - Dev",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn profiles_have_distinct_namespaces() {
        assert_eq!(AppProfile::Production.identifier(), "com.meowcal.sub");
        assert_eq!(AppProfile::Development.identifier(), "com.meowcal.sub.dev");
    }

    #[test]
    fn only_development_has_the_visible_suffix() {
        assert_eq!(AppProfile::Production.display_name(), "Meowcal Sub");
        assert_eq!(AppProfile::Development.display_name(), "Meowcal Sub - Dev");
    }

    #[test]
    fn current_profile_matches_the_build_kind() {
        let expected = if cfg!(debug_assertions) {
            AppProfile::Development
        } else {
            AppProfile::Production
        };
        assert_eq!(AppProfile::current(), expected);
    }
}
