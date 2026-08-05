use super::*;

fn default_root() -> PathBuf {
    PathBuf::from(r"C:\Users\someone\AppData\Local\meowcal\cache")
}

#[test]
fn a_recorded_root_is_tried_before_the_default() {
    let roots = candidate_roots(Some(r"D:\foundry-cache"), &default_root());

    assert_eq!(roots[0], PathBuf::from(r"D:\foundry-cache"));
    assert_eq!(roots[1], default_root());
}

#[test]
fn the_default_root_is_the_only_candidate_without_a_record() {
    assert_eq!(candidate_roots(None, &default_root()), vec![default_root()]);
}

// Installs predating `engine_cache_root` write an empty string rather than
// omitting the field; that must not become a candidate path of "".
// A root on a drive that is no longer attached would be handed to the
// installer verbatim, which then fails at `create_dir_all` with a raw OS
// error instead of falling back to the default.
#[test]
fn a_root_on_a_missing_drive_is_not_used() {
    let config = FoundryLocalConfig {
        engine_cache_root: Some(r"Q:\foundry-cache".to_string()),
        ..FoundryLocalConfig::default()
    };

    assert_eq!(install_cache_root(&config), None);
}

// ...but a root on a drive that is present is used even before its directory
// exists, which is the normal shape of a first install.
#[test]
fn a_root_on_a_present_drive_is_used() {
    let present = std::env::temp_dir().join("meowcal-not-created-yet");
    let config = FoundryLocalConfig {
        engine_cache_root: Some(present.to_string_lossy().to_string()),
        ..FoundryLocalConfig::default()
    };

    assert_eq!(
        install_cache_root(&config).as_deref(),
        Some(present.to_string_lossy().as_ref())
    );
}

#[test]
fn a_blank_recorded_root_is_ignored() {
    assert_eq!(
        candidate_roots(Some("   "), &default_root()),
        vec![default_root()]
    );
}

#[test]
fn the_default_root_is_not_listed_twice() {
    let recorded = default_root().to_string_lossy().to_string();
    assert_eq!(
        candidate_roots(Some(&recorded), &default_root()),
        vec![default_root()]
    );
}

// An engine the app is already using must not be replaced by whatever a
// directory scan happens to find.
#[test]
fn a_registered_runtime_is_left_alone() {
    let manifest = EngineManifest::shipped().expect("shipped manifest");
    let mut config = FoundryLocalConfig {
        managed_runtime: Some(crate::engine_config::ManagedLocalRuntimeConfig {
            kind: "hy-mt".to_string(),
            executable_path: r"D:\engine\llama-server.exe".to_string(),
            model_path: r"D:\engine\HY-MT.gguf".to_string(),
            port: 11_436,
        }),
        ..FoundryLocalConfig::default()
    };

    assert!(restore_registration(&mut config, &manifest, &default_root()).is_none());
}

// Configs written before `engine_cache_root` existed carry a runtime record
// and no recorded root. Without backfilling, the very installs this was
// built to rescue would still have nowhere to look.
#[test]
fn an_existing_install_gets_its_root_recorded() {
    let mut config = FoundryLocalConfig {
        managed_runtime: Some(crate::engine_config::ManagedLocalRuntimeConfig {
            kind: "hy-mt".to_string(),
            executable_path: r"D:\foundry-cache\meowcal-sub\runtime\engine\llama-server.exe"
                .to_string(),
            model_path: r"D:\foundry-cache\meowcal-sub\models\hy-mt\model.gguf".to_string(),
            port: 11_436,
        }),
        ..FoundryLocalConfig::default()
    };

    assert!(backfill_cache_root(&mut config));
    assert_eq!(
        config.engine_cache_root.as_deref(),
        Some(r"D:\foundry-cache")
    );
}

// Backfilling twice must not churn the config or trigger a pointless save.
#[test]
fn a_recorded_root_is_not_backfilled_again() {
    let mut config = FoundryLocalConfig {
        engine_cache_root: Some(r"D:\foundry-cache".to_string()),
        ..FoundryLocalConfig::default()
    };

    assert!(!backfill_cache_root(&mut config));
}

// Adoption launches the executable, so a file that is merely the right size
// must not be trusted. The installer hashes before registering; so does this.
#[test]
fn a_same_sized_but_wrong_install_is_not_adopted() {
    let manifest = EngineManifest::shipped().expect("shipped manifest");
    let runtime = manifest
        .runtime_for_current_arch()
        .expect("runtime for this arch");
    let cache_root = std::env::temp_dir().join("meowcal-recovery-tampered");
    let _ = std::fs::remove_dir_all(&cache_root);
    let paths = HyMtInstallPaths::from_cache_root(&cache_root, &manifest, runtime);

    // Right sizes, wrong bytes - what a corrupted or swapped artifact looks
    // like to a size check.
    for (file, size) in [
        (&paths.executable, runtime.executable.size_bytes),
        (&paths.model, manifest.model.artifact.size_bytes),
    ] {
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(file, vec![0u8; size as usize]).unwrap();
    }
    assert!(
        paths.is_complete(&manifest, runtime),
        "the fixture must pass the size check to prove the hash is what rejects it"
    );

    let mut config = FoundryLocalConfig {
        engine_cache_root: Some(cache_root.to_string_lossy().to_string()),
        ..FoundryLocalConfig::default()
    };
    let adopted = restore_registration(&mut config, &manifest, &default_root());

    let _ = std::fs::remove_dir_all(&cache_root);
    assert!(adopted.is_none());
    assert!(config.managed_runtime.is_none());
}

// Issue #65's core case, inverted: with nothing installed anywhere, recovery
// must decline rather than register paths that do not exist.
#[test]
fn nothing_is_adopted_when_no_candidate_holds_an_install() {
    let manifest = EngineManifest::shipped().expect("shipped manifest");
    let empty = std::env::temp_dir().join("meowcal-recovery-empty");
    let mut config = FoundryLocalConfig::default();

    assert!(restore_registration(&mut config, &manifest, &empty).is_none());
    assert!(config.managed_runtime.is_none());
}

// An older build can leave `engineCacheRoot` present but empty. Returning early
// on that left custom-root installs with nothing for recovery to search.
#[test]
fn a_blank_recorded_root_is_backfilled_like_a_missing_one() {
    let mut config = FoundryLocalConfig {
        engine_cache_root: Some("   ".to_string()),
        managed_runtime: Some(crate::engine_config::ManagedLocalRuntimeConfig {
            kind: "hy-mt".to_string(),
            executable_path: r"D:\foundry-cache\meowcal-sub\runtime\e\llama-server.exe".to_string(),
            model_path: r"D:\foundry-cache\meowcal-sub\models\m\model.gguf".to_string(),
            port: 11_436,
        }),
        ..FoundryLocalConfig::default()
    };

    assert!(backfill_cache_root(&mut config));
    assert_eq!(
        config.engine_cache_root.as_deref(),
        Some(r"D:\foundry-cache")
    );
}
