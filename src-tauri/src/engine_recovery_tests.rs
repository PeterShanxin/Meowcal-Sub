use super::*;

#[test]
fn candidates_prefer_and_deduplicate_the_recorded_root() {
    let default = PathBuf::from(r"C:\cache");
    assert_eq!(
        candidate_roots(Some(r"D:\models"), &default),
        vec![PathBuf::from(r"D:\models"), default.clone()]
    );
    assert_eq!(candidate_roots(Some(r"C:\cache"), &default), vec![default]);
}

#[test]
fn core_install_path_is_persisted_as_the_unversioned_base() {
    let paths = HyMtInstallPaths {
        root: PathBuf::from(r"D:\shared\production\0.1.0\x86_64"),
        runtime_dir: PathBuf::new(),
        runtime_archive: PathBuf::new(),
        executable: PathBuf::new(),
        model_dir: PathBuf::new(),
        model: PathBuf::new(),
    };
    assert_eq!(cache_root_of(&paths).as_deref(), Some(r"D:\shared"));
}

#[test]
fn recorded_legacy_root_becomes_an_unverified_migration_record() {
    let mut config = FoundryLocalConfig {
        engine_cache_root: Some(r"D:\old-cache".to_string()),
        ..FoundryLocalConfig::default()
    };
    assert!(add_migration_record(&mut config, Path::new(r"C:\default")));
    let runtime = config.managed_runtime.expect("migration record");
    assert!(runtime
        .executable_path
        .contains(r"D:\old-cache\meowcal-sub"));
}
