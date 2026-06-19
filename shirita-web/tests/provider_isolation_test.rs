//! Per-source provider config: switching the active source must not lose the
//! other sources' keys/models.

use std::sync::Arc;

use shirita_core::{SqliteStorage, Storage};

async fn storage() -> Arc<dyn Storage> {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("provider_iso.db");
    std::mem::forget(dir);
    let s = SqliteStorage::connect(path.to_str().unwrap()).await.unwrap();
    s.run_migrations().await.unwrap();
    Arc::new(s)
}

#[tokio::test]
async fn provider_config_is_per_source() {
    let s = storage().await;
    s.set_setting("provider_source", &serde_json::json!("openai")).await.unwrap();
    s.set_setting("provider.openai.api_key", &serde_json::json!("KEY_A")).await.unwrap();
    s.set_setting("provider.anthropic.api_key", &serde_json::json!("KEY_B")).await.unwrap();

    let (_src, _url, key, _model) = shirita_web::resolve_provider_config(s.as_ref()).await;
    assert_eq!(key, "KEY_A");

    s.set_setting("provider_source", &serde_json::json!("anthropic")).await.unwrap();
    let (_src, _url, key2, _model) = shirita_web::resolve_provider_config(s.as_ref()).await;
    assert_eq!(key2, "KEY_B"); // switching source does not lose the other's key
}

#[tokio::test]
async fn legacy_flat_keys_migrate_into_active_namespace() {
    let s = storage().await;
    // legacy install: only flat keys, no source set (defaults to openai)
    s.set_setting("provider_api_key", &serde_json::json!("LEGACY")).await.unwrap();
    s.set_setting("provider_model", &serde_json::json!("gpt-4o-mini")).await.unwrap();

    let (source, _url, key, model) = shirita_web::resolve_provider_config(s.as_ref()).await;
    assert_eq!(source, "openai");
    assert_eq!(key, "LEGACY");
    assert_eq!(model, "gpt-4o-mini");
    // migrated into the namespace so later reads are stable
    let ns = s.get_setting("provider.openai.api_key").await.unwrap();
    assert_eq!(ns.unwrap().as_str().unwrap(), "LEGACY");
}
