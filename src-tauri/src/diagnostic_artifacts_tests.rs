use super::*;

fn directory() -> PathBuf {
    let root = std::env::temp_dir().join(format!(
        "meowcal-diagnostics-test-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir(&root).unwrap();
    root
}

#[test]
fn storage_is_bounded_during_a_running_session() -> io::Result<()> {
    let root = directory();
    write_record(&root, b"12345", Some(b"abcde"), 20)?;
    let first = records(&root)?[0].0.clone();
    std::thread::sleep(std::time::Duration::from_millis(1));
    write_record(&root, b"12345", Some(b"abcde"), 20)?;
    std::thread::sleep(std::time::Duration::from_millis(1));
    write_record(&root, b"12345", Some(b"abcde"), 20)?;
    assert!(!first.exists());
    assert_eq!(
        records(&root)?.iter().map(|(_, size)| size).sum::<u64>(),
        20
    );
    std::fs::remove_dir_all(root)
}

#[test]
fn oversized_records_do_not_evict_existing_evidence() -> io::Result<()> {
    let root = directory();
    write_record(&root, b"kept", None, 10)?;
    assert!(write_record(&root, b"longer than allowed", None, 10).is_err());
    assert_eq!(records(&root)?.len(), 1);
    std::fs::remove_dir_all(root)
}

#[test]
fn unrelated_files_and_modified_records_are_preserved() -> io::Result<()> {
    let root = directory();
    std::fs::write(root.join("notes.txt"), b"user data")?;
    write_record(&root, b"record", None, 10)?;
    let record = records(&root)?[0].0.clone();
    std::fs::write(record.join("original.jpg"), b"user data")?;
    assert!(write_record(&root, b"new record", None, 10).is_err());
    assert_eq!(std::fs::read(root.join("notes.txt"))?, b"user data");
    assert_eq!(std::fs::read(record.join("original.jpg"))?, b"user data");
    std::fs::remove_dir_all(root)
}

#[test]
fn simultaneous_writers_keep_both_records() -> io::Result<()> {
    let root = directory();
    write_record(&root, b"kept", None, 20)?;
    let lock = std::fs::OpenOptions::new()
        .write(true)
        .open(root.join(".writer.lock"))?;
    lock.lock()?;
    let other_root = root.clone();
    let writer = std::thread::spawn(move || write_record(&other_root, b"new record", None, 20));
    drop(lock);
    writer.join().unwrap()?;
    assert_eq!(records(&root)?.len(), 2);
    std::fs::remove_dir_all(root)
}

#[test]
fn ownership_names_are_narrow() {
    assert!(owned_name("record-000123-42-1"));
    for name in [
        "notes",
        "record-notes",
        "record-1-2-",
        "record-1-2-3-backup",
    ] {
        assert!(!owned_name(name));
    }
}

#[cfg(unix)]
#[test]
fn links_are_not_followed_or_removed() -> io::Result<()> {
    let root = directory();
    let external = directory();
    std::fs::write(external.join("record.json"), b"keep")?;
    std::os::unix::fs::symlink(&external, root.join("record-1-2-3"))?;
    write_record(&root, b"new", None, 3)?;
    assert_eq!(std::fs::read(external.join("record.json"))?, b"keep");
    assert!(write_record(&root.join("record-1-2-3"), b"new", None, 3).is_err());
    std::fs::remove_dir_all(root)?;
    std::fs::remove_dir_all(external)
}
