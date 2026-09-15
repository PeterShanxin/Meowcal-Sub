use super::*;
use sha2::{Digest, Sha256};
use std::io::Write;

#[test]
fn runtime_tree_rejects_modified_and_extra_dlls() {
    let root = std::env::temp_dir().join(format!(
        "core-tree-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let tree = root.join("runtime");
    std::fs::create_dir_all(&tree).unwrap();
    let archive = root.join("runtime.zip");
    let mut zip = zip::ZipWriter::new(File::create(&archive).unwrap());
    for (name, bytes) in [
        ("llama-server.exe", b"exe".as_slice()),
        ("engine.dll", b"dll".as_slice()),
    ] {
        zip.start_file(name, zip::write::SimpleFileOptions::default())
            .unwrap();
        zip.write_all(bytes).unwrap();
        std::fs::write(tree.join(name), bytes).unwrap();
    }
    zip.finish().unwrap();
    assert!(verify_runtime_tree(&archive, &tree).is_ok());
    std::fs::write(tree.join("engine.dll"), b"bad").unwrap();
    assert!(verify_runtime_tree(&archive, &tree).is_err());
    std::fs::write(tree.join("engine.dll"), b"dll").unwrap();
    std::fs::write(tree.join("unexpected.dll"), b"bad").unwrap();
    assert!(verify_runtime_tree(&archive, &tree).is_err());
    std::fs::remove_file(tree.join("unexpected.dll")).unwrap();
    let mut duplicate = zip::ZipWriter::new(File::create(&archive).unwrap());
    for name in ["engine.dll", "ENGINE.DLL"] {
        duplicate
            .start_file(name, zip::write::SimpleFileOptions::default())
            .unwrap();
        duplicate.write_all(b"dll").unwrap();
    }
    duplicate.finish().unwrap();
    assert_eq!(
        verify_runtime_tree(&archive, &tree).unwrap_err(),
        "CORE_ARCHIVE_UNSAFE_PATH"
    );
    std::fs::remove_dir_all(root).unwrap();
}

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
    let current = HyMtInstallPaths::from_cache_root(root.join("0.1.1"), &manifest, runtime);
    let previous_root = root.join("0.1.0");
    let previous = HyMtInstallPaths::from_cache_root(&previous_root, &manifest, runtime);
    let roots = vec![previous_root];

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

#[tokio::test]
async fn legacy_copy_is_verified_independent_and_preserves_source() {
    let root = std::env::temp_dir().join(format!(
        "core-copy-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&root).unwrap();
    let source = root.join("legacy.bin");
    let target = root.join("core").join("asset.bin");
    std::fs::write(&source, b"asset").unwrap();
    let hash = format!("{:x}", Sha256::digest(b"asset"));
    copy_verified_source(&source, &target, 5, &hash)
        .await
        .unwrap();
    assert_eq!(std::fs::read(&target).unwrap(), b"asset");
    std::fs::write(&source, b"other").unwrap();
    assert_eq!(std::fs::read(&target).unwrap(), b"asset");
    let refused = root.join("refused.bin");
    assert!(copy_verified_source(&source, &refused, 5, &hash)
        .await
        .unwrap_err()
        .starts_with("CORE_IMPORT_CHANGED"));
    assert!(!refused.exists());
    std::fs::write(&refused, b"previous").unwrap();
    assert!(copy_verified_source(&source, &refused, 5, &hash)
        .await
        .is_err());
    assert_eq!(std::fs::read(&refused).unwrap(), b"previous");
    assert!(source.exists());
    std::fs::remove_dir_all(root).unwrap();
}
