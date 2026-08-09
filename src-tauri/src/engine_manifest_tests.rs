use super::*;
#[test]
fn shipped_manifest_represents_both_supported_architectures() {
    let manifest = EngineManifest::shipped().expect("shipped manifest should validate");
    assert_eq!(manifest.schema_version, 1);
    assert!(manifest.runtime_for(Architecture::Aarch64).is_ok());
    assert!(manifest.runtime_for(Architecture::X86_64).is_ok());
    assert_eq!(manifest.launch.host, "127.0.0.1");
    assert!(manifest.launch.extra_args.contains(&"--parallel".into()));
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
// Per-runtime launch args are optional (backwards compatible) and
// validated like shared launch args: absent field parses to empty, an
// empty-string argument is rejected.
#[test]
fn runtime_launch_args_are_optional_and_validated() {
    let without_field =
        SHIPPED_MANIFEST.replacen("      \"launchArgs\": [\"--no-kv-offload\"],\n", "", 1);
    let manifest = EngineManifest::parse(&without_field).expect("field should be optional");
    assert!(manifest
        .runtime_for(Architecture::Aarch64)
        .expect("aarch64 runtime should exist")
        .launch_args
        .is_empty());
    let empty_arg = SHIPPED_MANIFEST.replacen(
        "      \"launchArgs\": [\"--no-kv-offload\"],\n",
        "      \"launchArgs\": [\"  \"],\n",
        1,
    );
    assert!(matches!(
        EngineManifest::parse(&empty_arg),
        Err(ManifestError::Invalid(_))
    ));
}
