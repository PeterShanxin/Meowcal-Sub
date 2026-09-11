use super::*;
use crate::engine_manifest::EngineManifest;
use sha2::{Digest, Sha256};
use std::io::Write;
use std::time::{SystemTime, UNIX_EPOCH};
use zip::write::SimpleFileOptions;

#[tokio::test]
async fn a_real_archive_is_extracted() {
    let root = fixture_path("extract", "dir");
    let archive = root.join("runtime.zip");
    let destination = root.join("destination");
    std::fs::create_dir_all(&root).unwrap();
    write_archive(&archive, "bin/llama-server.exe", b"payload");

    extract_zip(&archive, &destination)
        .await
        .expect("valid archive should extract");

    assert_eq!(
        std::fs::read(destination.join("bin/llama-server.exe")).unwrap(),
        b"payload"
    );
    std::fs::remove_dir_all(root).ok();
}

#[tokio::test]
async fn an_unsafe_archive_path_is_rejected() {
    let root = fixture_path("extract-unsafe", "dir");
    let archive = root.join("runtime.zip");
    let destination = root.join("destination");
    std::fs::create_dir_all(&root).unwrap();
    write_archive(&archive, "../outside.txt", b"payload");

    let error = extract_zip(&archive, &destination)
        .await
        .expect_err("archive traversal must fail");

    assert!(error.starts_with("ENGINE_EXTRACT_FAILED:"), "{error}");
    assert!(error.contains("unsafe archive path"), "{error}");
    assert!(!root.join("outside.txt").exists());
    std::fs::remove_dir_all(root).ok();
}

#[tokio::test]
async fn duplicate_case_insensitive_archive_paths_are_rejected() {
    let root = fixture_path("extract-duplicate", "dir");
    let archive = root.join("runtime.zip");
    let destination = root.join("destination");
    std::fs::create_dir_all(&root).unwrap();
    let file = std::fs::File::create(&archive).unwrap();
    let mut writer = zip::ZipWriter::new(file);
    writer
        .start_file("bin/Runtime.dll", SimpleFileOptions::default())
        .unwrap();
    writer.write_all(b"first").unwrap();
    writer
        .start_file("BIN/runtime.DLL", SimpleFileOptions::default())
        .unwrap();
    writer.write_all(b"second").unwrap();
    writer.finish().unwrap();

    let error = extract_zip(&archive, &destination)
        .await
        .expect_err("case-insensitive duplicate archive paths must fail");

    assert!(error.contains("duplicate archive path"), "{error}");
    std::fs::remove_dir_all(root).ok();
}

#[tokio::test]
async fn a_failing_extraction_names_the_archive_and_reason() {
    let missing = fixture_path("absent", "zip");
    let destination = std::env::temp_dir();

    let error = extract_zip(&missing, &destination)
        .await
        .expect_err("extracting an absent archive must fail");

    assert!(error.starts_with("ENGINE_EXTRACT_FAILED:"), "{error}");
    assert!(error.contains(&missing.display().to_string()), "{error}");
    assert!(error.contains("could not open archive"), "{error}");
}

fn write_archive(path: &Path, name: &str, payload: &[u8]) {
    let file = std::fs::File::create(path).unwrap();
    let mut writer = zip::ZipWriter::new(file);
    writer
        .start_file(name, SimpleFileOptions::default())
        .unwrap();
    writer.write_all(payload).unwrap();
    writer.finish().unwrap();
}

#[tokio::test]
async fn same_sized_corrupt_artifact_is_not_treated_as_installed() {
    let path = fixture_path("integrity", "bin");
    std::fs::write(&path, b"trusted").unwrap();
    let expected_hash = digest_file_hex(&path).unwrap();
    assert!(file_matches(&path, 7, &expected_hash).await.unwrap());
    std::fs::write(&path, b"corrupt").unwrap();
    assert!(!file_matches(&path, 7, &expected_hash).await.unwrap());
    std::fs::remove_file(path).unwrap();
}

#[tokio::test]
async fn corrupt_runtime_executable_requires_repair() {
    let manifest = EngineManifest::shipped().unwrap();
    let runtime = manifest.runtime_for_current_arch().unwrap();
    let path = fixture_path("runtime-corrupt", "exe");
    std::fs::write(&path, vec![0; runtime.executable.size_bytes as usize]).unwrap();
    assert!(!file_matches(
        &path,
        runtime.executable.size_bytes,
        &runtime.executable.sha256
    )
    .await
    .unwrap());
    std::fs::remove_file(path).unwrap();
}

// Synchronous recovery may run on a 1 MiB Windows main-thread stack, so hashing
// must fit comfortably inside a smaller stack.
#[test]
fn blocking_verification_fits_in_a_small_thread_stack() {
    let path = fixture_path("small-stack", "bin");
    std::fs::write(&path, b"trusted").unwrap();
    let expected_hash = format!("{:x}", Sha256::digest(b"trusted"));

    let verified_path = path.clone();
    let matched = std::thread::Builder::new()
        .stack_size(256 * 1024)
        .spawn(move || file_matches_blocking(&verified_path, 7, &expected_hash))
        .unwrap()
        .join()
        .unwrap();

    std::fs::remove_file(path).unwrap();
    assert!(matched);
}

fn fixture_path(label: &str, extension: &str) -> std::path::PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!(
        "meowcal-{label}-{}-{unique}.{extension}",
        std::process::id()
    ))
}
