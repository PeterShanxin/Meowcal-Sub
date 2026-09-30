use super::tests::hold_readiness;
use super::*;

fn ready_test_status() -> CoreStatus {
    CoreStatus {
        installed: true,
        ready: true,
        cpu_locked: false,
        model: "test".into(),
        version: CORE_VERSION.into(),
        storage_root: PathBuf::new(),
        managed_config: None,
        install_paths: None,
    }
}

#[test]
fn queued_same_storage_install_blocks_focus_and_start_even_after_old_status() {
    use crate::llm::{FoundryLocalBackend, ReadyState, TranslatorBackend};
    let _readiness = hold_readiness();
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let _entered = runtime.enter();
    let old = ready_test_status();
    readiness::CACHE.lock().unwrap().status = Some(old.clone());
    let generation = readiness::generation().unwrap();
    let backend = FoundryLocalBackend::new(crate::config::FoundryLocalConfig {
        managed_runtime: Some(crate::config::ManagedLocalRuntimeConfig {
            kind: "hy-mt".into(),
            executable_path: "unused".into(),
            model_path: "unused".into(),
            port: 0,
        }),
        ..Default::default()
    });
    let active = TRANSLATION.get_or_init(|| Mutex::new(None)).lock().unwrap();
    // Installation is admitted before the blocking worker can acquire the channel.
    let install = call_async::<CoreStatus>(
        &TRANSLATION,
        "install",
        json!({}),
        Duration::from_secs(1),
        None,
        false,
    );
    assert!(readiness::installing());
    readiness::record(old, generation);
    assert!(!cached_status().unwrap().ready);
    assert_eq!(backend.ready_state(), ReadyState::NotReady);
    assert!(matches!(
        request::poll_status(
            TRANSLATION.get().unwrap(),
            &TRANSLATION_KILL,
            Duration::from_secs(1)
        )
        .unwrap(),
        StatusPoll::Busy
    ));
    assert!(ready_blocking(Duration::from_secs(1))
        .unwrap_err()
        .starts_with("CORE_INSTALL_BUSY"));
    drop(active);
    // No runtime is registered: failure must release the install guard, never readiness.
    assert!(runtime.block_on(install).is_err());
    assert!(!readiness::installing());
    assert!(!cached_status().unwrap().ready);
    readiness::CACHE.lock().unwrap().status = None;
}

#[test]
fn installation_lifetime_rejects_status_from_before_and_during_overlapping_installs() {
    let _readiness = hold_readiness();
    {
        let old = ready_test_status();
        readiness::CACHE.lock().unwrap().status = Some(old.clone());
        let before = readiness::generation().unwrap();
        let first = readiness::Install::begin().unwrap();
        let during = readiness::generation().unwrap();
        let second = readiness::Install::begin().unwrap();
        readiness::record(old.clone(), before);
        readiness::record(old.clone(), during);
        assert!(!cached_status().unwrap().ready);
        drop(first);
        assert!(readiness::installing());
        drop(second);
        assert!(!readiness::installing());
        readiness::record(old.clone(), during);
        assert!(!cached_status().unwrap().ready);
        // A fresh status may restore readiness after all installs have finished.
        readiness::record(old, readiness::generation().unwrap());
        assert!(cached_status().unwrap().ready);
    }
    readiness::CACHE.lock().unwrap().status = None;
}

#[test]
fn cancelled_queued_install_releases_guard_without_restoring_readiness() {
    let _readiness = hold_readiness();
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let _entered = runtime.enter();
    readiness::CACHE.lock().unwrap().status = Some(ready_test_status());
    let active = TRANSLATION.get_or_init(|| Mutex::new(None)).lock().unwrap();
    let mut install = Box::pin(call_async::<CoreStatus>(
        &TRANSLATION,
        "install",
        json!({}),
        Duration::from_secs(1),
        None,
        false,
    ));
    let mut context = std::task::Context::from_waker(std::task::Waker::noop());
    assert!(std::future::Future::poll(install.as_mut(), &mut context).is_pending());
    assert!(readiness::installing());
    drop(install);
    drop(active);
    for _ in 0..200 {
        if !readiness::installing() {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(!readiness::installing());
    assert!(!cached_status().unwrap().ready);
    readiness::CACHE.lock().unwrap().status = None;
}

#[test]
fn dispatched_install_suppresses_old_readiness_on_success_and_failure() {
    let _readiness = hold_readiness();
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let _entered = runtime.enter();
    for mode in ["install-success", "install-failure"] {
        let process = transport::tests::fixture(mode).unwrap();
        *TRANSLATION.get_or_init(|| Mutex::new(None)).lock().unwrap() = Some(process);
        let _cleanup = scopeguard::guard((), |_| {
            let mut slot = TRANSLATION.get().unwrap().lock().unwrap();
            clear_process(&mut slot, &TRANSLATION_KILL);
            readiness::CACHE.lock().unwrap().status = None;
        });
        let old = ready_test_status();
        readiness::CACHE.lock().unwrap().status = Some(old.clone());
        let before = readiness::generation().unwrap();
        let dispatched = Arc::new(AtomicBool::new(false));
        let notified = dispatched.clone();
        let progress = Arc::new(move |message: String| {
            assert_eq!(message, "install-dispatched");
            notified.store(true, Ordering::SeqCst);
            readiness::record(old.clone(), before);
            assert!(!cached_status().unwrap().ready);
            assert!(readiness::installing());
        });
        let result = runtime.block_on(install(progress));
        assert!(dispatched.load(Ordering::SeqCst));
        if mode == "install-success" {
            assert!(!result.unwrap().ready);
        } else {
            assert!(result.unwrap_err().starts_with("CORE_INSTALL_FAILED"));
        }
        assert!(!readiness::installing());
        assert!(!cached_status().unwrap().ready);
    }
}

#[test]
fn in_flight_status_cannot_restore_ready_when_install_is_admitted() {
    let _readiness = hold_readiness();
    for finish_install_first in [false, true] {
        readiness::CACHE.lock().unwrap().status = Some(ready_test_status());
        let slot = Arc::new(Mutex::new(Some(
            transport::tests::fixture("status-delayed").unwrap(),
        )));
        let reader_slot = slot.clone();
        let reader = std::thread::spawn(move || {
            request::poll_status(&reader_slot, &TRANSLATION_KILL, Duration::from_secs(2))
        });
        for _ in 0..200 {
            if slot.try_lock().is_err() {
                break;
            }
            std::thread::sleep(Duration::from_millis(1));
        }
        assert!(slot.try_lock().is_err(), "status request must be in flight");
        let install = readiness::Install::begin().unwrap();
        if finish_install_first {
            drop(install);
        } else {
            let _install = install;
            assert!(matches!(reader.join().unwrap().unwrap(), StatusPoll::Busy));
            assert!(!cached_status().unwrap().ready);
            if let Some(process) = slot.lock().unwrap().as_mut() {
                process.kill_and_wait();
            }
            continue;
        }
        assert!(matches!(reader.join().unwrap().unwrap(), StatusPoll::Busy));
        assert!(!cached_status().unwrap().ready);
        if let Some(process) = slot.lock().unwrap().as_mut() {
            process.kill_and_wait();
        };
    }
    readiness::CACHE.lock().unwrap().status = None;
}
