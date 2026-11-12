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
    write_record(&root, b"12345", Some(b"abcde"), 20, || true)?;
    let first = records(&root)?[0].0.clone();
    std::thread::sleep(std::time::Duration::from_millis(1));
    write_record(&root, b"12345", Some(b"abcde"), 20, || true)?;
    std::thread::sleep(std::time::Duration::from_millis(1));
    write_record(&root, b"12345", Some(b"abcde"), 20, || true)?;
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
    write_record(&root, b"kept", None, 10, || true)?;
    assert!(write_record(&root, b"longer than allowed", None, 10, || true).is_err());
    assert_eq!(records(&root)?.len(), 1);
    std::fs::remove_dir_all(root)
}

#[test]
fn unrelated_files_and_modified_records_are_preserved() -> io::Result<()> {
    let root = directory();
    std::fs::write(root.join("notes.txt"), b"user data")?;
    write_record(&root, b"record", None, 10, || true)?;
    let record = records(&root)?[0].0.clone();
    std::fs::write(record.join("original.jpg"), b"user data")?;
    assert!(write_record(&root, b"new record", None, 10, || true).is_err());
    assert_eq!(std::fs::read(root.join("notes.txt"))?, b"user data");
    assert_eq!(std::fs::read(record.join("original.jpg"))?, b"user data");
    std::fs::remove_dir_all(root)
}

#[test]
fn simultaneous_writers_keep_both_records() -> io::Result<()> {
    let root = directory();
    write_record(&root, b"kept", None, 20, || true)?;
    let lock = std::fs::OpenOptions::new()
        .write(true)
        .open(root.join(".writer.lock"))?;
    lock.lock()?;
    let other_root = root.clone();
    let writer = std::thread::spawn(move || {
        write_record(&other_root, b"new record", None, 20, || true)
    });
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
    write_record(&root, b"new", None, 3, || true)?;
    assert_eq!(std::fs::read(external.join("record.json"))?, b"keep");
    assert!(write_record(&root.join("record-1-2-3"), b"new", None, 3, || true).is_err());
    std::fs::remove_dir_all(root)?;
    std::fs::remove_dir_all(external)
}

#[test]
fn capture_png_is_opaque_and_preserves_rgb() {
    let png = encode_frame(vec![0, 0, 255, 255, 255, 0, 0, 0], 2, 1).unwrap();
    let image = image::load_from_memory(&png).unwrap().to_rgba8();
    assert_eq!(image.dimensions(), (2, 1));
    assert_eq!(image.into_raw(), vec![255, 0, 0, 255, 0, 0, 255, 255]);
    assert!(encode_frame(vec![0; 3], 1, 1).is_err());
    assert!(encode_frame(Vec::new(), 0, 0).is_err());
    assert!(encode_frame(Vec::new(), u32::MAX, u32::MAX).is_err());
}

#[tokio::test]
async fn cancellation_does_not_release_an_active_writer_slot() {
    let slots = Arc::new(Semaphore::new(1));
    let (started_tx, started_rx) = tokio::sync::oneshot::channel();
    let (release_tx, release_rx) = std::sync::mpsc::channel();
    let task = tokio::spawn(offload(Arc::clone(&slots), move || {
        let _ = started_tx.send(());
        release_rx.recv().map_err(io::Error::other)?;
        Ok(())
    }));
    started_rx.await.unwrap();
    task.abort();
    let _ = task.await;
    assert_eq!(slots.available_permits(), 0);
    release_tx.send(()).unwrap();
    let permit = tokio::time::timeout(std::time::Duration::from_secs(2), slots.acquire())
        .await
        .unwrap()
        .unwrap();
    drop(permit);
    assert_eq!(slots.available_permits(), 1);
}

#[test]
fn disabling_before_write_preserves_existing_evidence() -> io::Result<()> {
    let root = directory();
    write_record(&root, b"kept", None, 5, || true)?;
    write_record(&root, b"new", None, 5, || false)?;
    let existing = records(&root)?;
    assert_eq!(existing.len(), 1);
    assert_eq!(std::fs::read(existing[0].0.join("record.json"))?, b"kept");
    std::fs::remove_dir_all(root)
}

#[test]
fn a_stalled_external_writer_has_a_bounded_wait() -> io::Result<()> {
    let root = directory();
    write_record(&root, b"kept", None, 10, || true)?;
    let owner = std::fs::OpenOptions::new()
        .write(true)
        .open(root.join(".writer.lock"))?;
    owner.lock()?;
    let contender = std::fs::OpenOptions::new()
        .write(true)
        .open(root.join(".writer.lock"))?;
    assert!(lock_writer(&contender, std::time::Duration::ZERO).is_err());
    drop(contender);
    drop(owner);
    std::fs::remove_dir_all(root)
}
