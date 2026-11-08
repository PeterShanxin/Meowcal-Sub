use super::*;

fn request(id: &str) -> SelectionRequest {
    SelectionRequest {
        request_id: id.into(),
        start_translation: true,
    }
}

fn region() -> CaptureRegion {
    CaptureRegion::new(10, 800, 1200, 120)
}

#[tokio::test]
async fn unchanged_rectangle_requires_and_accepts_explicit_confirmation() {
    let requests = SelectionRequests::default();
    let mut receiver = requests.begin(request("first"), || 17).expect("open");
    assert_eq!(
        receiver.try_recv(),
        Err(oneshot::error::TryRecvError::Empty)
    );
    requests
        .complete("first", Some(region()), Some(17), |_| async { Ok(()) })
        .await
        .expect("confirm");
    assert_eq!(receiver.try_recv().expect("outcome").region, Some(region()));
    assert!(requests.current().is_none());
}

#[tokio::test]
async fn stale_and_duplicate_confirmations_do_not_complete_a_new_request() {
    let requests = SelectionRequests::default();
    let mut first = requests.begin(request("first"), || 17).expect("first");
    requests
        .complete("first", None, None, |_| async { Ok(()) })
        .await
        .expect("cancel");
    assert_eq!(first.try_recv().expect("cancelled").region, None);
    let mut second = requests.begin(request("second"), || 17).expect("second");
    assert!(requests
        .complete("first", Some(region()), Some(17), |_| async {
            panic!("stale apply")
        })
        .await
        .is_err());
    assert_eq!(second.try_recv(), Err(oneshot::error::TryRecvError::Empty));
    requests
        .complete("second", Some(region()), Some(17), |_| async { Ok(()) })
        .await
        .expect("confirm");
    assert_eq!(second.try_recv().expect("confirmed").request_id, "second");
    assert!(requests
        .complete("second", Some(region()), Some(17), |_| async {
            panic!("duplicate apply")
        })
        .await
        .is_err());
}

#[tokio::test]
async fn failed_validation_or_hide_keeps_selection_pending_for_retry() {
    let requests = SelectionRequests::default();
    let mut receiver = requests.begin(request("first"), || 17).expect("open");
    assert_eq!(
        requests
            .complete("first", Some(region()), Some(17), |_| async {
                Err("hide failed".into())
            })
            .await,
        Err("hide failed".into())
    );
    assert_eq!(requests.current(), Some(request("first")));
    assert_eq!(
        receiver.try_recv(),
        Err(oneshot::error::TryRecvError::Empty)
    );
    requests
        .complete("first", None, None, |_| async { Ok(()) })
        .await
        .expect("retry cancel");
    assert_eq!(receiver.try_recv().expect("cancelled").region, None);
}

#[tokio::test]
async fn confirmation_is_not_delivered_until_apply_has_finished() {
    let requests = SelectionRequests::default();
    let mut receiver = requests.begin(request("first"), || 17).expect("open");
    requests
        .complete("first", Some(region()), Some(17), |_| async {
            assert_eq!(
                receiver.try_recv(),
                Err(oneshot::error::TryRecvError::Empty)
            );
            Ok(())
        })
        .await
        .expect("confirm");
    assert_eq!(
        receiver.try_recv().expect("confirmed").region,
        Some(region())
    );
}

#[tokio::test]
async fn opening_another_selector_does_not_replace_the_pending_request() {
    let requests = SelectionRequests::default();
    let _receiver = requests.begin(request("first"), || 17).expect("first");
    assert!(requests.begin(request("second"), || 17).is_err());
    assert_eq!(requests.current(), Some(request("first")));
}

#[tokio::test]
async fn malformed_request_ids_are_rejected() {
    let requests = SelectionRequests::default();
    for id in [String::new(), "x".repeat(129)] {
        assert!(requests.begin(request(&id), || 17).is_err());
    }
    assert!(requests.current().is_none());
}

#[tokio::test]
async fn selection_wire_contract_names_request_and_start_intent() {
    assert_eq!(
        serde_json::to_value(request("first")).expect("request"),
        serde_json::json!({
            "requestId": "first", "startTranslation": true,
        })
    );
    assert_eq!(
        serde_json::to_value(SelectionOutcome {
            request_id: "first".into(),
            region: None
        })
        .expect("cancelled"),
        serde_json::json!({
            "requestId": "first", "region": null,
        })
    );
}

#[tokio::test]
async fn a_foreign_or_missing_native_token_does_not_commit() {
    let requests = SelectionRequests::default();
    let mut receiver = requests.begin(request("first"), || 17).expect("open");
    for token in [None, Some(18)] {
        assert!(requests
            .complete("first", Some(region()), token, |_| async {
                panic!("invalid token committed");
            })
            .await
            .is_err());
        assert_eq!(
            receiver.try_recv(),
            Err(oneshot::error::TryRecvError::Empty)
        );
    }
    requests
        .complete("first", Some(region()), Some(17), |token| async move {
            assert_eq!(token, 17);
            Ok(())
        })
        .await
        .expect("matching token");
    assert_eq!(
        receiver.try_recv().expect("confirmed").region,
        Some(region())
    );
}

#[tokio::test]
async fn persistence_failure_keeps_the_request_retryable_without_an_outcome() {
    let requests = SelectionRequests::default();
    let mut receiver = requests.begin(request("first"), || 17).expect("open");
    assert!(requests
        .complete("first", Some(region()), Some(17), |_| async {
            tokio::task::yield_now().await;
            Err("config write failed".into())
        })
        .await
        .is_err());
    assert_eq!(
        receiver.try_recv(),
        Err(oneshot::error::TryRecvError::Empty)
    );
    assert_eq!(requests.current(), Some(request("first")));
    requests
        .complete("first", None, None, |_| async { Ok(()) })
        .await
        .expect("cancel");
    assert_eq!(receiver.try_recv().expect("cancelled").region, None);
}

#[tokio::test]
async fn concurrent_completions_wait_for_persistence_and_apply_only_once() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;
    let requests = Arc::new(SelectionRequests::default());
    let mut receiver = requests.begin(request("first"), || 17).expect("open");
    let writes = Arc::new(AtomicUsize::new(0));
    let (entered, started) = oneshot::channel();
    let (release, released) = oneshot::channel();
    let first_requests = Arc::clone(&requests);
    let first_writes = Arc::clone(&writes);
    let first = tokio::spawn(async move {
        first_requests
            .complete("first", Some(region()), Some(17), |_| async move {
                first_writes.fetch_add(1, Ordering::SeqCst);
                entered.send(()).expect("started");
                released.await.expect("release persistence");
                Ok(())
            })
            .await
    });
    started.await.expect("persistence entered");
    let duplicate_requests = Arc::clone(&requests);
    let duplicate_writes = Arc::clone(&writes);
    let duplicate = tokio::spawn(async move {
        duplicate_requests
            .complete("first", Some(region()), Some(17), |_| async move {
                duplicate_writes.fetch_add(1, Ordering::SeqCst);
                Ok(())
            })
            .await
    });
    tokio::task::yield_now().await;
    assert_eq!(requests.current(), Some(request("first")));
    assert_eq!(
        receiver.try_recv(),
        Err(oneshot::error::TryRecvError::Empty)
    );
    assert!(!duplicate.is_finished());
    release.send(()).expect("release");
    first.await.expect("first task").expect("first confirm");
    assert!(duplicate.await.expect("duplicate task").is_err());
    assert_eq!(writes.load(Ordering::SeqCst), 1);
    assert_eq!(
        receiver.try_recv().expect("confirmed").region,
        Some(region())
    );
}
