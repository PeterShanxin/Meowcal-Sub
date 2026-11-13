use super::*;
use crate::http_server::{create_router, HttpAppState};
use crate::ui_language::UiLanguage;
use axum_test::TestServer;
use std::sync::Arc;

fn state(config: AppConfig, config_path: PathBuf) -> HttpAppState {
    HttpAppState {
        config: Arc::new(Mutex::new(config)),
        settings_save: Arc::new(tokio::sync::Mutex::new(())),
        capture_region: Arc::new(Mutex::new(None)),
        translation_diagnostics: Arc::new(Mutex::new(Default::default())),
        config_path,
    }
}

#[test]
fn http_language_edits_are_durable_and_stale_settings_cannot_revert_them() {
    let _guard = provenance_lock();
    remember_provenance(Provenance::Authoritative);
    let path = temp_dir("ui-language-route").join("config.json");
    let initial = AppConfig {
        source_language: "ja-JP".into(),
        target_language: "en-US".into(),
        ..registered()
    };
    let server = TestServer::new(create_router(state(initial.clone(), path.clone())));
    tokio_test::block_on(async {
        server
            .post("/api/ui-language")
            .json(&serde_json::json!({"language":"zh-CN"}))
            .await
            .assert_status_ok();
        let saved: AppConfig = server.get("/api/settings").await.json();
        assert_eq!(saved.ui_language, UiLanguage::SimplifiedChinese);
        assert_eq!(saved.source_language, "ja-JP");
        assert_eq!(saved.target_language, "en-US");
        assert!(saved.translation.foundry_local.managed_runtime.is_some());

        let mut stale = initial.clone();
        stale.overlay.font_size = 38;
        server
            .post("/api/settings")
            .json(&stale)
            .await
            .assert_status_ok();
        let after_stale: AppConfig = server.get("/api/settings").await.json();
        assert_eq!(after_stale.ui_language, UiLanguage::SimplifiedChinese);
        assert_eq!(after_stale.overlay.font_size, 38);

        server
            .post("/api/ui-language")
            .json(&serde_json::json!({"language":"fr"}))
            .await
            .assert_status(axum::http::StatusCode::UNPROCESSABLE_ENTITY);
        let after_invalid: AppConfig = server.get("/api/settings").await.json();
        assert_eq!(after_invalid.ui_language, UiLanguage::SimplifiedChinese);
    });
    let LoadOutcome::Loaded(on_disk) = read_from(&path) else {
        panic!("saved settings");
    };
    assert_eq!(on_disk.ui_language, UiLanguage::SimplifiedChinese);
    assert_eq!(on_disk.overlay.font_size, 38);
}

#[test]
fn concurrent_http_language_and_stale_settings_edits_preserve_both_intents() {
    let _guard = provenance_lock();
    remember_provenance(Provenance::Authoritative);
    let path = temp_dir("ui-language-concurrent").join("config.json");
    let initial = AppConfig::default();
    let server = TestServer::new(create_router(state(initial.clone(), path.clone())));
    tokio_test::block_on(async {
        for preference in ["zh-CN", "en", "system"] {
            let mut stale = initial.clone();
            stale.overlay.font_size = 36;
            let language = server
                .post("/api/ui-language")
                .json(&serde_json::json!({"language":preference}));
            let settings = server.post("/api/settings").json(&stale);
            let (language_response, settings_response) = tokio::join!(language, settings);
            language_response.assert_status_ok();
            settings_response.assert_status_ok();
            let saved: serde_json::Value = server.get("/api/settings").await.json();
            assert_eq!(saved["uiLanguage"], preference);
            assert_eq!(saved["overlay"]["fontSize"], 36);
            let LoadOutcome::Loaded(on_disk) = read_from(&path) else {
                panic!("saved settings");
            };
            assert_eq!(
                serde_json::to_value(on_disk).unwrap()["uiLanguage"],
                preference
            );
        }
    });
}

#[test]
fn a_failed_http_language_save_preserves_the_live_preference() {
    let _guard = provenance_lock();
    remember_provenance(Provenance::Authoritative);
    let path = temp_dir("ui-language-failed").join("config.json");
    fs::create_dir(&path).unwrap();
    let server = TestServer::new(create_router(state(AppConfig::default(), path)));
    tokio_test::block_on(async {
        server
            .post("/api/ui-language")
            .json(&serde_json::json!({"language":"zh-CN"}))
            .await
            .assert_status(axum::http::StatusCode::INTERNAL_SERVER_ERROR);
        let unchanged: AppConfig = server.get("/api/settings").await.json();
        assert_eq!(unchanged.ui_language, UiLanguage::System);
    });
}
