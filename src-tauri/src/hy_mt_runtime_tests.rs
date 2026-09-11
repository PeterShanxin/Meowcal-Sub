use super::*;

#[test]
fn legacy_endpoint_is_only_a_stable_migration_value() {
    let runtime = ManagedLocalRuntimeConfig {
        kind: "hy-mt".to_string(),
        executable_path: r"C:\legacy\server.exe".to_string(),
        model_path: r"C:\legacy\model.gguf".to_string(),
        port: 11_436,
    };
    assert_eq!(endpoint_url(&runtime), "http://127.0.0.1:11436");
}
