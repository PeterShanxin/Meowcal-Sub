use super::*;
use crate::sha256::encode_hex;
use crate::storage::Lease;
use sha2::{Digest, Sha256};
use std::path::PathBuf;

fn temporary_base() -> PathBuf {
    std::env::temp_dir().join(format!(
        "core-reclaim-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ))
}

fn test_manifest() -> EngineManifest {
    let mut manifest = EngineManifest::shipped().unwrap();
    for runtime in &mut manifest.runtimes {
        runtime.executable.size_bytes = 3;
    }
    manifest.model.artifact.size_bytes = 5;
    manifest.model.artifact.sha256 = encode_hex(&Sha256::digest(b"model").into());
    manifest
}

fn install(
    base: &Path,
    parts: &[&str],
    manifest: &EngineManifest,
    model: &[u8],
) -> HyMtInstallPaths {
    let root = parts
        .iter()
        .fold(base.to_path_buf(), |path, part| path.join(part))
        .join(std::env::consts::ARCH);
    let paths = HyMtInstallPaths::from_cache_root(
        root,
        manifest,
        manifest.runtime_for_current_arch().unwrap(),
    );
    std::fs::create_dir_all(&paths.model_dir).unwrap();
    std::fs::write(&paths.model, model).unwrap();
    paths
}

/// A completed install: the executable exists and the install state records it.
async fn complete(base: &Path, parts: &[&str], manifest: &EngineManifest) -> HyMtInstallPaths {
    let paths = install(base, parts, manifest, b"model");
    let runtime = manifest.runtime_for_current_arch().unwrap();
    std::fs::create_dir_all(&paths.runtime_dir).unwrap();
    std::fs::write(&paths.executable, b"exe").unwrap();
    let record =
        crate::engine_install_transaction::InstalledEngine::from_install(&paths, manifest, runtime)
            .unwrap();
    crate::engine_install_transaction::record_active(&paths.root, record)
        .await
        .unwrap();
    paths
}

#[tokio::test]
async fn reclaim_keeps_the_newest_complete_version_and_shares_identical_models() {
    let base = temporary_base();
    let manifest = test_manifest();
    let current = complete(&base, &["sub1", "production", "0.1.5"], &manifest).await;
    // A start that failed before installing leaves only the lease file.
    let abandoned = base
        .join("sub1/production/0.1.4")
        .join(std::env::consts::ARCH);
    drop(
        Lease::acquire(&abandoned, false, std::time::Duration::ZERO)
            .await
            .unwrap(),
    );
    let previous = complete(&base, &["sub1", "production", "0.1.3"], &manifest).await;
    let stale = complete(&base, &["sub1", "production", "0.1.2"], &manifest).await;
    let running = complete(&base, &["sub1", "production", "0.1.1"], &manifest).await;
    let other_client = install(&base, &["sub2", "production", "0.1.5"], &manifest, b"model");
    let unpartitioned = install(&base, &["production", "0.1.0"], &manifest, b"model");
    let different = install(&base, &["production", "0.0.9"], &manifest, b"mode!");
    let busy = install(&base, &["sub2", "production", "0.1.0"], &manifest, b"model");
    let mut leases = Vec::new();
    for leased in [&running, &busy] {
        leases.push(
            Lease::acquire(&leased.root, false, std::time::Duration::ZERO)
                .await
                .unwrap(),
        );
    }

    reclaim(&current, &manifest).await;

    assert!(!base.join("sub1/production/0.1.2").exists());
    assert!(!stale.root.exists());
    assert!(!abandoned.exists());
    assert!(previous.model.exists());
    assert!(running.model.exists());
    assert!(!same_file(&current.model, &running.model).unwrap());
    assert!(!same_file(&current.model, &busy.model).unwrap());
    for shared in [&previous, &other_client, &unpartitioned] {
        assert!(same_file(&current.model, &shared.model).unwrap());
    }
    assert!(!same_file(&current.model, &different.model).unwrap());
    assert_eq!(std::fs::read(&different.model).unwrap(), b"mode!");
    assert_eq!(std::fs::read(&current.model).unwrap(), b"model");
    drop(leases);
    std::fs::remove_dir_all(base).unwrap();
}
