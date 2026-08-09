use super::*;
use crate::engine_manifest::Architecture;
#[test]
fn install_layout_is_stable_and_port_is_local_only() {
    let manifest = EngineManifest::shipped().expect("manifest should be valid");
    let package = manifest
        .runtime_for_current_arch()
        .expect("current architecture should be supported");
    let paths = HyMtInstallPaths::from_cache_root(r"D:\model-cache", &manifest, package);
    let runtime = paths.managed_config(&manifest);
    assert!(paths.model.ends_with(&manifest.model.artifact.file_name));
    assert!(paths.executable.ends_with("llama-server.exe"));
    assert_eq!(endpoint_url(&runtime), "http://127.0.0.1:11436");
    assert!(package.archive.size_bytes > 0);
    assert_eq!(package.archive.sha256.len(), 64);
}
#[test]
fn release_asset_matches_current_windows_architecture() {
    let manifest = EngineManifest::shipped().expect("manifest should be valid");
    let package = manifest
        .runtime_for_current_arch()
        .expect("current architecture should be supported");
    assert!(package.archive.file_name.ends_with(".zip"));
    assert!(package.archive.url.contains(&package.archive.file_name));
    assert!(package.executable.relative_path.ends_with(".exe"));
    assert_eq!(package.gpu_layers, 99);
}
// Manifest contract (arch-independent so CI on either architecture guards
// it): aarch64 is the GPU candidate - full-layer Adreno offload with KV
// pinned to the CPU (`-ngl 99 --no-kv-offload`; plain KV offload hangs on
// the tested b10155 + Adreno/OpenCL combination, measured 2026-08-09) -
// while x64 Vulkan stays exactly as shipped.
#[test]
fn runtime_acceleration_contract_per_architecture() {
    let manifest = EngineManifest::shipped().expect("manifest should be valid");
    let aarch64 = manifest
        .runtime_for(Architecture::Aarch64)
        .expect("aarch64 runtime should exist");
    assert_eq!(aarch64.acceleration, "gpu");
    assert_eq!(aarch64.gpu_layers, 99);
    assert_eq!(aarch64.launch_args, vec!["--no-kv-offload".to_string()]);
    let x64 = manifest
        .runtime_for(Architecture::X86_64)
        .expect("x86_64 runtime should exist");
    assert_eq!(x64.acceleration, "vulkan");
    assert_eq!(x64.gpu_layers, 99);
    assert!(x64.launch_args.is_empty());
}
// The exact argument vector handed to llama-server carries `-ngl 99` and
// the runtime-specific tail for aarch64 on a validated host, and never
// leaks `--no-kv-offload` into the x64 line.
#[test]
fn launch_arguments_forward_runtime_specific_args() {
    let manifest = EngineManifest::shipped().expect("manifest should be valid");
    let aarch64 = manifest
        .runtime_for(Architecture::Aarch64)
        .expect("aarch64 runtime should exist");
    let x64 = manifest
        .runtime_for(Architecture::X86_64)
        .expect("x86_64 runtime should exist");
    let runtime = ManagedLocalRuntimeConfig {
        kind: "hy-mt".to_string(),
        executable_path: r"C:\engines\llama-server.exe".to_string(),
        model_path: r"C:\models\HY-MT1.5-1.8B-Q4_K_M.gguf".to_string(),
        port: 11436,
    };
    let aarch64_policy = effective_launch_policy(aarch64, true, false);
    assert!(aarch64_policy.gpu_active);
    let aarch64_args = launch_arguments(&runtime, &manifest, &aarch64_policy, "11436");
    assert!(aarch64_args.windows(2).any(|pair| pair == ["-ngl", "99"]));
    assert!(aarch64_args.contains(&"--no-kv-offload".to_string()));
    // The runtime-specific flag must come after the shared manifest args.
    let no_kv = aarch64_args
        .iter()
        .position(|arg| arg == "--no-kv-offload")
        .expect("no-kv-offload should be present");
    let jinja = aarch64_args
        .iter()
        .position(|arg| arg == "--jinja")
        .expect("manifest jinja arg should be present");
    assert!(no_kv > jinja);
    let x64_args = launch_arguments(
        &runtime,
        &manifest,
        &effective_launch_policy(x64, false, false),
        "11436",
    );
    assert!(!x64_args.contains(&"--no-kv-offload".to_string()));
    assert!(x64_args.windows(2).any(|pair| pair == ["-ngl", "99"]));
}
// The compatibility decision, arch-independent: the Adreno GPU policy
// applies only on the validated GPU without a CPU override; unvalidated
// hardware and the startup fallback both get exactly the pre-GPU CPU policy
// (`-ngl 0`, no KV flag); the x64 Vulkan runtime is never gated.
#[test]
fn effective_policy_gates_the_adreno_gpu_path() {
    let manifest = EngineManifest::shipped().expect("manifest should be valid");
    let aarch64 = manifest
        .runtime_for(Architecture::Aarch64)
        .expect("aarch64 runtime should exist");
    let x64 = manifest
        .runtime_for(Architecture::X86_64)
        .expect("x86_64 runtime should exist");

    let validated = effective_launch_policy(aarch64, true, false);
    assert_eq!(validated.gpu_layers, 99);
    assert_eq!(validated.launch_args, vec!["--no-kv-offload".to_string()]);
    assert!(validated.gpu_active);

    for (adreno_validated, force_cpu) in [(false, false), (true, true), (false, true)] {
        let policy = effective_launch_policy(aarch64, adreno_validated, force_cpu);
        assert_eq!(policy.gpu_layers, 0, "{adreno_validated}/{force_cpu}");
        assert!(
            policy.launch_args.is_empty(),
            "{adreno_validated}/{force_cpu}"
        );
        assert!(!policy.gpu_active, "{adreno_validated}/{force_cpu}");
    }

    // x64 is not the gated runtime: gate inputs must not touch its policy.
    for (adreno_validated, force_cpu) in [(false, false), (true, false), (true, true)] {
        let policy = effective_launch_policy(x64, adreno_validated, force_cpu);
        assert_eq!(policy.gpu_layers, 99, "{adreno_validated}/{force_cpu}");
        assert!(!policy.gpu_active, "{adreno_validated}/{force_cpu}");
    }
}
// A gated-out or fallback CPU launch must produce the CPU argument line:
// zero layers and no KV constraint, so the engine runs the pre-GPU CPU
// configuration rather than a half-GPU one.
#[test]
fn cpu_policy_produces_the_cpu_launch_line() {
    let manifest = EngineManifest::shipped().expect("manifest should be valid");
    let aarch64 = manifest
        .runtime_for(Architecture::Aarch64)
        .expect("aarch64 runtime should exist");
    let runtime = ManagedLocalRuntimeConfig {
        kind: "hy-mt".to_string(),
        executable_path: r"C:\engines\llama-server.exe".to_string(),
        model_path: r"C:\models\HY-MT1.5-1.8B-Q4_K_M.gguf".to_string(),
        port: 11436,
    };
    let policy = effective_launch_policy(aarch64, false, false);
    let args = launch_arguments(&runtime, &manifest, &policy, "11436");
    assert!(args.windows(2).any(|pair| pair == ["-ngl", "0"]));
    assert!(!args.contains(&"--no-kv-offload".to_string()));
}

#[test]
fn validated_gpu_keeps_normal_policy_and_gets_measured_startup_headroom() {
    let manifest = EngineManifest::shipped().expect("manifest should be valid");
    let aarch64 = manifest
        .runtime_for(Architecture::Aarch64)
        .expect("aarch64 runtime should exist");
    let policy = effective_launch_policy(aarch64, true, false);
    let started = Instant::now();
    let deadline = started + Duration::from_secs(90);

    let attempt_deadline = readiness_deadline(deadline, started, policy.gpu_active);

    assert!(policy.gpu_active);
    assert_eq!(attempt_deadline, started + Duration::from_secs(30));
}

#[test]
fn failed_gpu_switches_to_cpu_with_remaining_time_under_one_deadline() {
    let manifest = EngineManifest::shipped().expect("manifest should be valid");
    let aarch64 = manifest
        .runtime_for(Architecture::Aarch64)
        .expect("aarch64 runtime should exist");
    let started = Instant::now();
    let deadline = started + Duration::from_secs(90);
    let gpu_deadline = readiness_deadline(deadline, started, true);
    let cpu_policy = effective_launch_policy(aarch64, true, true);
    let cpu_deadline = readiness_deadline(deadline, gpu_deadline, cpu_policy.gpu_active);

    assert_eq!(cpu_policy.gpu_layers, 0);
    assert!(!cpu_policy.gpu_active);
    assert_eq!(cpu_deadline, deadline);
    assert_eq!(
        cpu_deadline.saturating_duration_since(gpu_deadline),
        Duration::from_secs(60)
    );
    assert_eq!(
        deadline.saturating_duration_since(started),
        Duration::from_secs(90)
    );
}

#[test]
fn short_gpu_budget_is_split_and_cpu_only_paths_keep_the_full_deadline() {
    let manifest = EngineManifest::shipped().expect("manifest should be valid");
    let aarch64 = manifest
        .runtime_for(Architecture::Aarch64)
        .expect("aarch64 runtime should exist");
    let x64 = manifest
        .runtime_for(Architecture::X86_64)
        .expect("x86_64 runtime should exist");
    let started = Instant::now();
    let deadline = started + Duration::from_secs(20);

    let gpu_deadline = readiness_deadline(deadline, started, true);
    let unvalidated_arm = effective_launch_policy(aarch64, false, false);
    let x64_policy = effective_launch_policy(x64, false, false);
    let cpu_deadline = readiness_deadline(deadline, started, unvalidated_arm.gpu_active);
    let x64_deadline = readiness_deadline(deadline, started, x64_policy.gpu_active);

    assert_eq!(gpu_deadline, started + Duration::from_secs(10));
    assert_eq!(
        deadline.saturating_duration_since(gpu_deadline),
        Duration::from_secs(10)
    );
    assert_eq!(unvalidated_arm.gpu_layers, 0);
    assert_eq!(cpu_deadline, deadline);
    assert_eq!(x64_policy.gpu_layers, 99);
    assert_eq!(x64_deadline, deadline);
}

#[test]
fn occupied_preferred_port_selects_another_loopback_port() {
    let occupied =
        TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).expect("fixture listener should bind");
    let preferred = occupied
        .local_addr()
        .expect("fixture address should be available")
        .port();
    let selected = select_loopback_port(preferred).expect("fallback port should be selected");
    assert_ne!(selected, preferred);
    assert!(TcpListener::bind((Ipv4Addr::LOCALHOST, preferred)).is_err());
}
// Focused runtime proof for the P1 fallback: the CPU policy the fallback
// selects must launch the real engine and serve a real translation, not
// merely type-check. Opt-in because it needs an installed engine; run with
// MEOWCAL_HYMT_CACHE_ROOT pointing at a complete install:
//   MEOWCAL_HYMT_CACHE_ROOT=C:\path\to\foundry-cache cargo test --lib -- --ignored
#[tokio::test]
#[ignore = "requires an installed HY-MT engine (set MEOWCAL_HYMT_CACHE_ROOT)"]
async fn forced_cpu_launch_serves_a_real_translation() {
    use crate::config::FoundryLocalConfig;
    use crate::llm::{FoundryLocalBackend, TranslatorBackend};

    let Some(cache_root) = std::env::var_os("MEOWCAL_HYMT_CACHE_ROOT") else {
        eprintln!("MEOWCAL_HYMT_CACHE_ROOT not set; skipping");
        return;
    };
    let manifest = EngineManifest::shipped().expect("manifest should be valid");
    let package = manifest
        .runtime_for_current_arch()
        .expect("current architecture should be supported");
    let paths = HyMtInstallPaths::from_cache_root(cache_root, &manifest, package);
    if !paths.is_complete(&manifest, package) {
        eprintln!("no complete engine install under the cache root; skipping");
        return;
    }

    shutdown_owned();
    let runtime = paths.managed_config(&manifest);
    let timeout = Duration::from_secs(90);
    let endpoint = ensure_ready_with_policy(&runtime, Instant::now() + timeout, timeout, true)
        .await
        .expect("CPU policy should bring the engine up");
    let config = FoundryLocalConfig {
        model: Some(manifest.model.id.clone()),
        endpoint_url: Some(endpoint),
        managed_runtime: Some(runtime),
        timeout_ms: 90_000,
        ..FoundryLocalConfig::default()
    };
    let translated = FoundryLocalBackend::new(config)
        .translate("先不提时钟塔", "zh-CN", "en-US")
        .await
        .expect("CPU engine should answer a translation");
    shutdown_owned();
    assert!(!translated.trim().is_empty());
    assert_ne!(translated.trim(), "先不提时钟塔");
}
