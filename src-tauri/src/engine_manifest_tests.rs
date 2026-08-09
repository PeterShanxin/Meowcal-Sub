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
// Per-runtime launch args are optional (the x64 runtime ships without the
// field and validates) and an empty-string argument is rejected.
#[test]
fn runtime_launch_args_are_optional_and_validated() {
    let manifest = EngineManifest::shipped().expect("shipped manifest should validate");
    assert!(manifest
        .runtime_for(Architecture::X86_64)
        .expect("x64 runtime should exist")
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
// Unknown runtime fields are rejected, so a typo'd key cannot silently drop
// a policy field into its serde default (e.g. `launchArg` leaving
// launch_args empty on the Adreno runtime).
#[test]
fn unknown_runtime_fields_are_rejected() {
    let typo = SHIPPED_MANIFEST.replacen(
        "      \"launchArgs\": [\"--no-kv-offload\"],\n",
        "      \"launchArgs\": [\"--no-kv-offload\"],\n      \"launch_args\": [],\n",
        1,
    );
    assert!(matches!(
        EngineManifest::parse(&typo),
        Err(ManifestError::Invalid(_))
    ));
}
// llama.cpp honors the last occurrence of a repeated flag and per-runtime
// args are appended after every app-owned argument, so a runtime arg naming
// launcher-owned configuration would silently override it. Every form -
// separate value and `=`-joined, short and long alias - must be rejected.
// Each case keeps `--no-kv-offload` so rejection is attributable to the
// conflict rule, not the Adreno KV constraint.
#[test]
fn runtime_launch_args_cannot_override_app_owned_configuration() {
    for conflicting in [
        "[\"--no-kv-offload\", \"-m\", \"other.gguf\"]",
        "[\"--no-kv-offload\", \"--model\", \"other.gguf\"]",
        "[\"--no-kv-offload\", \"--alias\", \"other-model\"]",
        "[\"--no-kv-offload\", \"--host\", \"0.0.0.0\"]",
        "[\"--no-kv-offload\", \"--port=1\"]",
        "[\"--no-kv-offload\", \"-c\", \"512\"]",
        "[\"--no-kv-offload\", \"--ctx-size=512\"]",
        "[\"--no-kv-offload\", \"-ngl\", \"0\"]",
        "[\"--no-kv-offload\", \"--n-gpu-layers\", \"0\"]",
        "[\"--no-kv-offload\", \"--gpu-layers=0\"]",
        "[\"--no-kv-offload\", \"-t\", \"4\"]",
        "[\"--no-kv-offload\", \"--threads=4\"]",
        "[\"--no-kv-offload\", \"--parallel\", \"2\"]",
        "[\"--no-kv-offload\", \"-np\", \"2\"]",
    ] {
        let tampered = SHIPPED_MANIFEST.replacen("[\"--no-kv-offload\"]", conflicting, 1);
        assert!(
            matches!(
                EngineManifest::parse(&tampered),
                Err(ManifestError::Invalid(_))
            ),
            "{conflicting} should be rejected"
        );
    }
    // A benign runtime-specific flag still passes.
    let benign = SHIPPED_MANIFEST.replacen(
        "[\"--no-kv-offload\"]",
        "[\"--no-kv-offload\", \"--flash-attn\"]",
        1,
    );
    assert!(EngineManifest::parse(&benign).is_ok());
}
// Shared extra args may not override launcher-owned flags either. Two
// exemptions keep existing designed behavior: `--parallel` is the shared
// slot policy these args own, and a pinned thread count is detected and
// honored by `engine_launch::launch_args` as a deliberate choice.
#[test]
fn shared_extra_args_cannot_override_app_owned_configuration() {
    let conflict = SHIPPED_MANIFEST.replacen(
        "\"extraArgs\": [\"--jinja\", \"--no-webui\", \"--parallel\", \"1\"]",
        "\"extraArgs\": [\"--jinja\", \"--no-webui\", \"--host\", \"0.0.0.0\"]",
        1,
    );
    assert!(matches!(
        EngineManifest::parse(&conflict),
        Err(ManifestError::Invalid(_))
    ));
    let pinned_threads = SHIPPED_MANIFEST.replacen(
        "\"extraArgs\": [\"--jinja\", \"--no-webui\", \"--parallel\", \"1\"]",
        "\"extraArgs\": [\"--jinja\", \"--no-webui\", \"--parallel\", \"1\", \"--threads\", \"6\"]",
        1,
    );
    assert!(EngineManifest::parse(&pinned_threads).is_ok());
}
// On the shipped b10155 Adreno runtime, layer offload with the KV cache on
// the GPU is the measured hang configuration. The guard keys on the
// effective offload (gpuLayers), not the acceleration label, because the
// launcher passes `-ngl gpuLayers` regardless of the label: dropping
// `--no-kv-offload` while layers stay on the GPU must fail validation
// itself, not just a contract test. The rule is scoped to that runtime id:
// the same runtime configured for CPU, and any future runtime version,
// stay expressible.
#[test]
fn adreno_b10155_gpu_policy_requires_kv_cache_on_cpu() {
    let without_constraint =
        SHIPPED_MANIFEST.replacen("      \"launchArgs\": [\"--no-kv-offload\"],\n", "", 1);
    assert!(matches!(
        EngineManifest::parse(&without_constraint),
        Err(ManifestError::Invalid(_))
    ));
    let cpu_fallback = SHIPPED_MANIFEST
        .replacen("\"acceleration\": \"gpu\"", "\"acceleration\": \"cpu\"", 1)
        .replacen(
            "      \"gpuLayers\": 99,\n      \"launchArgs\": [\"--no-kv-offload\"],\n",
            "      \"gpuLayers\": 0,\n",
            1,
        );
    assert!(EngineManifest::parse(&cpu_fallback).is_ok());
}
// The exact Codex P2 counterexample: a "CPU fallback" edit that changes the
// label but forgets gpuLayers (and drops the KV flag) describes CPU but
// launches `-ngl 99` - the measured hang. Both halves of the incoherence
// must be rejected, in both directions.
#[test]
fn acceleration_label_and_layer_count_must_agree() {
    let label_only_cpu = SHIPPED_MANIFEST
        .replacen("\"acceleration\": \"gpu\"", "\"acceleration\": \"cpu\"", 1)
        .replacen("      \"launchArgs\": [\"--no-kv-offload\"],\n", "", 1);
    assert!(matches!(
        EngineManifest::parse(&label_only_cpu),
        Err(ManifestError::Invalid(_))
    ));
    let gpu_without_layers = SHIPPED_MANIFEST.replacen(
        "      \"gpuLayers\": 99,\n      \"launchArgs\": [\"--no-kv-offload\"],\n",
        "      \"gpuLayers\": 0,\n      \"launchArgs\": [\"--no-kv-offload\"],\n",
        1,
    );
    assert!(matches!(
        EngineManifest::parse(&gpu_without_layers),
        Err(ManifestError::Invalid(_))
    ));

    // A typo must not become an undocumented acceleration mode merely
    // because it happens to carry a non-zero layer count.
    let unknown_acceleration =
        SHIPPED_MANIFEST.replacen("\"acceleration\": \"gpu\"", "\"acceleration\": \"gup\"", 1);
    assert!(matches!(
        EngineManifest::parse(&unknown_acceleration),
        Err(ManifestError::Invalid(_))
    ));
}
