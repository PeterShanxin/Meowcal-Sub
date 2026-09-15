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
