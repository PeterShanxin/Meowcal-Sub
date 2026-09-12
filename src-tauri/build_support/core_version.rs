use serde_json::{Map, Value};
use std::fs;
use std::path::Path;

const LOCK_FIELDS: &[&str] = &[
    "schemaVersion",
    "repository",
    "tag",
    "coreVersion",
    "apiVersion",
    "architectures",
];
const CORE_REPOSITORY: &str = "PeterShanxin/Meowcal-Sub";

pub struct Selection {
    pub expected_version: String,
    pub source_candidate: bool,
}

pub fn resolve(
    repository_root: &Path,
    explicit_source_candidate: bool,
) -> Result<Selection, String> {
    let source_version = source_version(repository_root)?;
    let lock_path = repository_root.join("config/meowcal-core.lock.json");
    if explicit_source_candidate || !lock_path.is_file() {
        return Ok(Selection {
            expected_version: source_version,
            source_candidate: true,
        });
    }

    let contents = fs::read_to_string(&lock_path)
        .map_err(|error| format!("could not read {}: {error}", lock_path.display()))?;
    let lock: Value = serde_json::from_str(&contents)
        .map_err(|error| format!("{} is not valid JSON: {error}", lock_path.display()))?;
    let expected_version = validate_lock(&lock)?;
    Ok(Selection {
        expected_version,
        source_candidate: false,
    })
}

fn source_version(repository_root: &Path) -> Result<String, String> {
    let manifest_path = repository_root.join("core/Cargo.toml");
    let manifest = fs::read_to_string(&manifest_path)
        .map_err(|error| format!("could not read {}: {error}", manifest_path.display()))?;
    let mut in_package = false;
    for line in manifest.lines() {
        let line = line.trim();
        if line == "[package]" {
            in_package = true;
            continue;
        }
        if in_package && line.starts_with('[') {
            break;
        }
        if !in_package {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        if key.trim() != "version" {
            continue;
        }
        let version = value.trim().trim_matches('"');
        if is_semver(version) {
            return Ok(version.to_string());
        }
        return Err(
            "core/Cargo.toml must declare a major.minor.patch package version.".to_string(),
        );
    }
    Err("core/Cargo.toml must declare a major.minor.patch package version.".to_string())
}

fn validate_lock(lock: &Value) -> Result<String, String> {
    let lock = lock
        .as_object()
        .ok_or_else(|| "Core release lock must be an object.".to_string())?;
    exact_fields(lock, LOCK_FIELDS, "Core release lock")?;
    let version = string(lock, "coreVersion")?;
    if !is_semver(version) || string(lock, "tag")? != format!("core-v{version}") {
        return Err("Core release lock version and tag do not match.".to_string());
    }
    if number(lock, "schemaVersion")? != 1
        || string(lock, "repository")? != CORE_REPOSITORY
        || number(lock, "apiVersion")? != 1
    {
        return Err("Core release lock identity does not match schema 1 and API 1.".to_string());
    }

    let architectures = object(lock, "architectures")?;
    exact_fields(
        architectures,
        &["x64", "arm64"],
        "Core release lock architectures",
    )?;
    for architecture in ["x64", "arm64"] {
        let entry = object(architectures, architecture)?;
        exact_fields(
            entry,
            &["asset", "sha256"],
            "Core release lock architecture entry",
        )?;
        if string(entry, "asset")? != format!("meowcal-core-v{version}-windows-{architecture}.zip")
            || !is_sha256(string(entry, "sha256")?)
        {
            return Err(format!("Core {architecture} lock entry is invalid."));
        }
    }
    Ok(version.to_string())
}

fn exact_fields(object: &Map<String, Value>, expected: &[&str], name: &str) -> Result<(), String> {
    if object.len() != expected.len() || expected.iter().any(|field| !object.contains_key(*field)) {
        return Err(format!("{name} fields do not match schema 1."));
    }
    Ok(())
}

fn string<'a>(object: &'a Map<String, Value>, name: &str) -> Result<&'a str, String> {
    object
        .get(name)
        .and_then(Value::as_str)
        .ok_or_else(|| format!("Core release lock {name} must be a string."))
}

fn number(object: &Map<String, Value>, name: &str) -> Result<u64, String> {
    object
        .get(name)
        .and_then(Value::as_u64)
        .ok_or_else(|| format!("Core release lock {name} must be an integer."))
}

fn object<'a>(
    object: &'a Map<String, Value>,
    name: &str,
) -> Result<&'a Map<String, Value>, String> {
    object
        .get(name)
        .and_then(Value::as_object)
        .ok_or_else(|| format!("Core release lock {name} must be an object."))
}

fn is_semver(version: &str) -> bool {
    let segments = version.split('.').collect::<Vec<_>>();
    segments.len() == 3
        && segments
            .iter()
            .all(|segment| !segment.is_empty() && segment.bytes().all(|byte| byte.is_ascii_digit()))
}

fn is_sha256(digest: &str) -> bool {
    digest.len() == 64
        && digest != "0".repeat(64)
        && digest.bytes().all(|byte| {
            byte.is_ascii_digit() || (byte.is_ascii_lowercase() && byte.is_ascii_hexdigit())
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn fixture_root() -> std::path::PathBuf {
        let root = std::env::temp_dir().join(format!(
            "meowcal-core-version-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system clock")
                .as_nanos()
        ));
        fs::create_dir_all(root.join("core")).expect("create source manifest directory");
        fs::write(
            root.join("core/Cargo.toml"),
            "[package]\nname = \"meowcal-core\"\nversion = \"0.1.0\"\n",
        )
        .expect("write source manifest");
        root
    }

    fn write_lock(root: &Path, version: &str) {
        fs::create_dir_all(root.join("config")).expect("create lock directory");
        let asset =
            |architecture: &str| format!("meowcal-core-v{version}-windows-{architecture}.zip");
        let lock = serde_json::json!({
            "schemaVersion": 1,
            "repository": CORE_REPOSITORY,
            "tag": format!("core-v{version}"),
            "coreVersion": version,
            "apiVersion": 1,
            "architectures": {
                "x64": { "asset": asset("x64"), "sha256": "a".repeat(64) },
                "arm64": { "asset": asset("arm64"), "sha256": "b".repeat(64) }
            }
        });
        fs::write(
            root.join("config/meowcal-core.lock.json"),
            format!("{lock}\n"),
        )
        .expect("write release lock");
    }

    #[test]
    fn release_lock_supersedes_the_source_candidate() {
        let root = fixture_root();
        write_lock(&root, "0.1.1");
        let selection = resolve(&root, false).expect("release lock is valid");
        assert_eq!(selection.expected_version, "0.1.1");
        assert!(!selection.source_candidate);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn source_candidate_is_explicit_or_used_before_a_release_lock_exists() {
        let root = fixture_root();
        let absent_lock = resolve(&root, false).expect("source candidate without release lock");
        assert_eq!(absent_lock.expected_version, "0.1.0");
        assert!(absent_lock.source_candidate);

        write_lock(&root, "0.1.1");
        let explicit_source = resolve(&root, true).expect("explicit source candidate");
        assert_eq!(explicit_source.expected_version, "0.1.0");
        assert!(explicit_source.source_candidate);
        let _ = fs::remove_dir_all(root);
    }
}
