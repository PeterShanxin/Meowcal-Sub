use super::*;

fn temporary_base(label: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "core-partitions-{label}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ))
}

fn partition(base: &Path, parts: &[&str]) -> PathBuf {
    let path = parts
        .iter()
        .fold(base.to_path_buf(), |path, part| path.join(part))
        .join(std::env::consts::ARCH);
    std::fs::create_dir_all(&path).unwrap();
    path
}

#[test]
fn siblings_separate_this_clients_versions_from_shared_partitions() {
    let base = temporary_base("siblings");
    let current = partition(&base, &["sub1", "production", "0.1.4"]);
    let newer = partition(&base, &["sub1", "production", "0.1.10"]);
    let older = partition(&base, &["sub1", "production", "0.1.2"]);
    let other_client = partition(&base, &["sub2", "production", "0.1.5"]);
    let unpartitioned = partition(&base, &["production", "0.1.0"]);
    partition(&base, &["sub1", "development", "0.1.3"]);
    partition(&base, &["sub1", "production", "latest"]);
    std::fs::create_dir_all(base.join("sub1/production/0.1.1/other-arch")).unwrap();

    let siblings = sibling_partitions(&current);

    assert_eq!(siblings.own, vec![newer, older]);
    assert_eq!(siblings.shared, vec![other_client, unpartitioned]);
    std::fs::remove_dir_all(base).unwrap();
}

#[test]
fn a_root_outside_the_client_layout_has_no_siblings() {
    let base = temporary_base("unpartitioned");
    let root = partition(&base, &["production", "0.1.3"]);
    partition(&base, &["production", "0.1.2"]);

    let siblings = sibling_partitions(&root);

    assert!(siblings.own.is_empty());
    assert!(siblings.shared.is_empty());
    std::fs::remove_dir_all(base).unwrap();
}
