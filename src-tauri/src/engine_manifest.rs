use serde::Deserialize;
use std::collections::HashSet;
use std::path::{Component, Path};
use thiserror::Error;

const SUPPORTED_SCHEMA_VERSION: u32 = 1;
const SHIPPED_MANIFEST: &str = include_str!("../../config/engine-manifest.v1.json");

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EngineManifest {
    pub schema_version: u32,
    pub manifest_id: String,
    pub engine_version: String,
    pub minimum_app_version: String,
    pub authenticity: AuthenticityPolicy,
    pub model: ModelSpec,
    pub runtimes: Vec<RuntimeSpec>,
    pub requirements: SystemRequirements,
    pub launch: LaunchPolicy,
    pub rollback: RollbackPolicy,
    pub licenses: Vec<LicenseReference>,
    pub support_codes: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthenticityPolicy {
    pub mode: String,
    pub remote_refresh: bool,
    pub policy: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelSpec {
    pub id: String,
    pub name: String,
    pub quantization: String,
    pub install_directory: String,
    pub artifact: DownloadArtifact,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadArtifact {
    pub file_name: String,
    pub url: String,
    pub size_bytes: u64,
    pub sha256: String,
    pub license_id: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeSpec {
    pub id: String,
    pub architecture: Architecture,
    pub acceleration: String,
    pub install_directory: String,
    pub archive: DownloadArtifact,
    pub executable: InstalledExecutable,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstalledExecutable {
    pub relative_path: String,
    pub size_bytes: u64,
    pub sha256: String,
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq, Hash)]
pub enum Architecture {
    #[serde(rename = "aarch64")]
    Aarch64,
    #[serde(rename = "x86_64")]
    X86_64,
}

impl Architecture {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Aarch64 => "aarch64",
            Self::X86_64 => "x86_64",
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemRequirements {
    pub minimum_windows_build: u32,
    pub minimum_ram_bytes: u64,
    pub minimum_free_disk_bytes: u64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LaunchPolicy {
    pub host: String,
    pub preferred_port: u16,
    pub port_policy: String,
    pub context_size: u32,
    pub gpu_layers: u32,
    pub extra_args: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RollbackPolicy {
    pub retain_last_known_good: bool,
    pub minimum_engine_version: String,
    pub compatible_engine_versions: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LicenseReference {
    pub id: String,
    pub name: String,
    pub url: String,
    pub distribution_review: String,
}

#[derive(Debug, Clone, Error, PartialEq, Eq)]
pub enum ManifestError {
    #[error("ENGINE_MANIFEST_INVALID: {0}")]
    Invalid(String),
    #[error("ENGINE_UNSUPPORTED_ARCH: {0}")]
    UnsupportedArchitecture(String),
    #[error("ENGINE_ROLLBACK_REJECTED: {0}")]
    RollbackRejected(String),
}

impl EngineManifest {
    pub fn parse(json: &str) -> Result<Self, ManifestError> {
        let manifest: Self = serde_json::from_str(json)
            .map_err(|error| ManifestError::Invalid(error.to_string()))?;
        manifest.validate()?;
        Ok(manifest)
    }

    pub fn shipped() -> Result<Self, ManifestError> {
        Self::parse(SHIPPED_MANIFEST)
    }

    pub fn runtime_for(&self, architecture: Architecture) -> Result<&RuntimeSpec, ManifestError> {
        self.runtimes
            .iter()
            .find(|runtime| runtime.architecture == architecture)
            .ok_or_else(|| ManifestError::UnsupportedArchitecture(format!("{architecture:?}")))
    }

    pub fn runtime_for_current_arch(&self) -> Result<&RuntimeSpec, ManifestError> {
        self.runtime_for(current_architecture()?)
    }

    pub fn validate_transition(
        &self,
        current_version: &str,
        target_version: &str,
    ) -> Result<(), ManifestError> {
        let current = parse_version(current_version)?;
        let target = parse_version(target_version)?;
        if target >= current {
            return Ok(());
        }
        let minimum = parse_version(&self.rollback.minimum_engine_version)?;
        let allowed = self.rollback.retain_last_known_good
            && target >= minimum
            && self
                .rollback
                .compatible_engine_versions
                .iter()
                .any(|version| version == target_version);
        if allowed {
            Ok(())
        } else {
            Err(ManifestError::RollbackRejected(format!(
                "{current_version} -> {target_version}"
            )))
        }
    }

    fn validate(&self) -> Result<(), ManifestError> {
        if self.schema_version != SUPPORTED_SCHEMA_VERSION {
            return invalid(format!("unsupported schema {}", self.schema_version));
        }
        for value in [
            &self.manifest_id,
            &self.engine_version,
            &self.minimum_app_version,
            &self.model.id,
            &self.model.name,
            &self.model.quantization,
        ] {
            if value.trim().is_empty() {
                return invalid("required manifest identity is empty");
            }
        }
        parse_version(&self.engine_version)?;
        parse_version(&self.minimum_app_version)?;
        if self.authenticity.mode != "embeddedApplicationRelease"
            || self.authenticity.remote_refresh
            || self.authenticity.policy.trim().is_empty()
        {
            return invalid("unsigned remote manifest refresh is forbidden");
        }
        validate_relative_path(&self.model.install_directory)?;
        validate_artifact(&self.model.artifact)?;

        let mut architectures = HashSet::new();
        let mut runtime_ids = HashSet::new();
        for runtime in &self.runtimes {
            if !architectures.insert(runtime.architecture) {
                return invalid("duplicate runtime architecture");
            }
            if runtime.id.trim().is_empty() || !runtime_ids.insert(&runtime.id) {
                return invalid("duplicate or empty runtime id");
            }
            validate_relative_path(&runtime.install_directory)?;
            validate_artifact(&runtime.archive)?;
            validate_relative_path(&runtime.executable.relative_path)?;
            validate_size_hash(runtime.executable.size_bytes, &runtime.executable.sha256)?;
        }
        for required in [Architecture::Aarch64, Architecture::X86_64] {
            self.runtime_for(required)?;
        }
        if self.requirements.minimum_windows_build < 22_000
            || self.requirements.minimum_ram_bytes == 0
            || self.requirements.minimum_free_disk_bytes
                < self.model.artifact.size_bytes.saturating_mul(2)
        {
            return invalid("system requirements are incomplete");
        }
        if self.launch.host != "127.0.0.1"
            || self.launch.preferred_port == 0
            || self.launch.port_policy != "dynamicLoopback"
            || self.launch.context_size == 0
            || self
                .launch
                .extra_args
                .iter()
                .any(|arg| arg.trim().is_empty())
        {
            return invalid("launch policy is unsafe or incomplete");
        }
        let license_ids: HashSet<&str> = self
            .licenses
            .iter()
            .map(|license| license.id.as_str())
            .collect();
        if !license_ids.contains(self.model.artifact.license_id.as_str())
            || self
                .runtimes
                .iter()
                .any(|runtime| !license_ids.contains(runtime.archive.license_id.as_str()))
            || self.licenses.iter().any(|license| {
                license.name.trim().is_empty()
                    || !license.url.starts_with("https://")
                    || license.distribution_review.trim().is_empty()
            })
        {
            return invalid("artifact license reference is missing");
        }
        if !self.rollback.retain_last_known_good
            || !self
                .rollback
                .compatible_engine_versions
                .contains(&self.engine_version)
        {
            return invalid("last-known-good rollback policy is incomplete");
        }
        parse_version(&self.rollback.minimum_engine_version)?;
        if self.support_codes.is_empty()
            || self
                .support_codes
                .iter()
                .any(|code| !code.starts_with("ENGINE_"))
        {
            return invalid("support codes are incomplete");
        }
        Ok(())
    }
}

pub fn current_architecture() -> Result<Architecture, ManifestError> {
    match std::env::consts::ARCH {
        "aarch64" => Ok(Architecture::Aarch64),
        "x86_64" => Ok(Architecture::X86_64),
        other => Err(ManifestError::UnsupportedArchitecture(other.to_string())),
    }
}

fn validate_artifact(artifact: &DownloadArtifact) -> Result<(), ManifestError> {
    validate_relative_path(&artifact.file_name)?;
    if !artifact.url.starts_with("https://") || artifact.license_id.trim().is_empty() {
        return invalid("artifact URL or license is invalid");
    }
    validate_size_hash(artifact.size_bytes, &artifact.sha256)
}

fn validate_size_hash(size: u64, hash: &str) -> Result<(), ManifestError> {
    if size == 0 || hash.len() != 64 || !hash.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return invalid("artifact size or SHA-256 is invalid");
    }
    Ok(())
}

fn validate_relative_path(value: &str) -> Result<(), ManifestError> {
    let path = Path::new(value);
    if value.trim().is_empty()
        || path.is_absolute()
        || path
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return invalid("manifest path must be a safe relative path");
    }
    Ok(())
}

fn parse_version(value: &str) -> Result<[u64; 3], ManifestError> {
    let parts = value
        .split('.')
        .map(str::parse::<u64>)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| ManifestError::Invalid(format!("invalid version {value}")))?;
    if parts.len() != 3 {
        return invalid(format!("invalid version {value}"));
    }
    Ok([parts[0], parts[1], parts[2]])
}

fn invalid<T>(message: impl Into<String>) -> Result<T, ManifestError> {
    Err(ManifestError::Invalid(message.into()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shipped_manifest_represents_both_supported_architectures() {
        let manifest = EngineManifest::shipped().expect("shipped manifest should validate");
        assert_eq!(manifest.schema_version, 1);
        assert!(manifest.runtime_for(Architecture::Aarch64).is_ok());
        assert!(manifest.runtime_for(Architecture::X86_64).is_ok());
        assert_eq!(manifest.launch.host, "127.0.0.1");
        assert!(!manifest.authenticity.remote_refresh);
    }

    #[test]
    fn corrupt_and_unknown_manifests_are_rejected() {
        assert!(EngineManifest::parse("{").is_err());
        let unknown_schema =
            SHIPPED_MANIFEST.replacen("\"schemaVersion\": 1", "\"schemaVersion\": 99", 1);
        assert!(matches!(
            EngineManifest::parse(&unknown_schema),
            Err(ManifestError::Invalid(_))
        ));
        let unknown_arch = SHIPPED_MANIFEST.replacen("\"aarch64\"", "\"riscv64\"", 1);
        assert!(matches!(
            EngineManifest::parse(&unknown_arch),
            Err(ManifestError::Invalid(_))
        ));
    }

    #[test]
    fn invalid_hash_and_unsafe_path_are_rejected() {
        let invalid_hash = SHIPPED_MANIFEST.replacen(
            "4383ac0c3c8e476de98ff979c2a3f069f8c4fb385e7860cf2d28da896cc477c7",
            "not-a-sha",
            1,
        );
        assert!(EngineManifest::parse(&invalid_hash).is_err());
        let unsafe_path = SHIPPED_MANIFEST.replacen("\"hy-mt1.5-1.8b-q4\"", "\"../outside\"", 1);
        assert!(EngineManifest::parse(&unsafe_path).is_err());
    }

    #[test]
    fn downgrade_requires_an_explicit_compatible_rollback_target() {
        let manifest = EngineManifest::shipped().expect("shipped manifest should validate");
        assert!(manifest.validate_transition("1.0.0", "1.1.0").is_ok());
        assert!(manifest.validate_transition("1.0.0", "1.0.0").is_ok());
        assert!(manifest.validate_transition("1.1.0", "1.0.0").is_ok());
        assert!(matches!(
            manifest.validate_transition("2.0.0", "0.9.0"),
            Err(ManifestError::RollbackRejected(_))
        ));
    }
}
