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
// the runtime-specific tail for aarch64, and never leaks `--no-kv-offload`
// into the x64 line.
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
    let aarch64_args = launch_arguments(&runtime, &manifest, aarch64, "11436");
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
    let x64_args = launch_arguments(&runtime, &manifest, x64, "11436");
    assert!(!x64_args.contains(&"--no-kv-offload".to_string()));
    assert!(x64_args.windows(2).any(|pair| pair == ["-ngl", "99"]));
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
