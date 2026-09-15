use super::*;

#[test]
fn offline_assets_are_found_in_a_previous_core_install() {
    let root = std::env::temp_dir().join(format!(
        "core-offline-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let manifest = EngineManifest::parse(
        &include_str!("../config/engine-manifest.v1.json")
            .replacen("\"sizeBytes\": 1133080512", "\"sizeBytes\": 5", 1)
            .replacen("\"sizeBytes\": 12868798", "\"sizeBytes\": 3", 1)
            .replacen("\"sizeBytes\": 33576473", "\"sizeBytes\": 3", 1),
    )
    .unwrap();
    let runtime = manifest.runtime_for_current_arch().unwrap();
    let partition = |version: &str| {
        root.join("production")
            .join(version)
            .join(std::env::consts::ARCH)
    };
    let current = HyMtInstallPaths::from_cache_root(partition("0.1.2"), &manifest, runtime);
    let previous = HyMtInstallPaths::from_cache_root(partition("0.1.1"), &manifest, runtime);
    // The application passes the shared storage base, not the old partition.
    let roots = vec![root.clone()];

    assert!(!assets_available_offline(&current, &roots, &manifest));
    std::fs::create_dir_all(previous.runtime_archive.parent().unwrap()).unwrap();
    std::fs::write(&previous.runtime_archive, b"zip").unwrap();
    assert!(!assets_available_offline(&current, &roots, &manifest));
    std::fs::create_dir_all(&previous.model_dir).unwrap();
    std::fs::write(&previous.model, b"mode").unwrap();
    assert!(!assets_available_offline(&current, &roots, &manifest));
    std::fs::write(&previous.model, b"model").unwrap();
    assert!(assets_available_offline(&current, &roots, &manifest));
    std::fs::remove_dir_all(root).unwrap();
}
