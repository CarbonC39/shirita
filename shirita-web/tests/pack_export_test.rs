//! GET /api/packs/{id}/export — zip when the pack has binary, JSON otherwise.

use std::sync::Arc;

use axum::body::Body;
use axum::http::{header, HeaderMap, Request, StatusCode};
use http_body_util::BodyExt;
use tower::ServiceExt;

use shirita_core::{
    Config, EchoProvider, ModelProvider, Pack, SqliteStorage, Storage, TiktokenCounter, TokenCounter,
};
use shirita_web::{app, AppState};

async fn test_state(dir: &std::path::Path) -> AppState {
    let storage = SqliteStorage::connect(dir.join("p.db").to_str().unwrap()).await.unwrap();
    storage.run_migrations().await.unwrap();
    let storage: Arc<dyn Storage> = Arc::new(storage);
    let config = Arc::new(Config::new("ignored", dir.join("assets").to_str().unwrap(), "secret-token").unwrap());
    let provider: Arc<dyn ModelProvider> = Arc::new(EchoProvider);
    let token_counter: Arc<dyn TokenCounter> = Arc::new(TiktokenCounter::new());
    AppState { storage, config, provider, token_counter, model: "m".into(), generations: Arc::new(shirita_web::Generations::new()), http_client: shirita_web::new_http_client() }
}

async fn get(state: &AppState, uri: &str) -> (StatusCode, HeaderMap, Vec<u8>) {
    let req = Request::builder()
        .method("GET")
        .uri(uri)
        .header(header::AUTHORIZATION, "Bearer secret-token")
        .body(Body::empty())
        .unwrap();
    let res = app(state.clone()).oneshot(req).await.unwrap();
    let status = res.status();
    let headers = res.headers().clone();
    let body = res.into_body().collect().await.unwrap().to_bytes().to_vec();
    (status, headers, body)
}

#[tokio::test]
async fn export_pack_with_avatar_is_a_zip_bundle() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join("assets")).unwrap();
    std::fs::write(dir.path().join("assets/face.png"), b"img-bytes").unwrap();
    let state = test_state(dir.path()).await;

    let mut pack = Pack::new("Alice");
    pack.identity.avatar = Some("face.png".into());
    state.storage.create_pack(&pack).await.unwrap();

    let (st, headers, body) = get(&state, &format!("/api/packs/{}/export", pack.id)).await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(headers.get(header::CONTENT_TYPE).unwrap(), "application/zip");
    assert!(headers.get(header::CONTENT_DISPOSITION).unwrap().to_str().unwrap().ends_with(".zip\""));
    assert_eq!(&body[..2], b"PK"); // zip local-file magic

    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(body)).unwrap();
    let names: Vec<String> = (0..zip.len()).map(|i| zip.by_index(i).unwrap().name().to_string()).collect();
    assert!(names.contains(&"manifest.json".to_string()));
    assert!(names.contains(&"assets/face.png".to_string()));
}

#[tokio::test]
async fn export_pack_without_binary_is_json() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join("assets")).unwrap();
    let state = test_state(dir.path()).await;

    let pack = Pack::new("Bob"); // no avatar, no panel
    state.storage.create_pack(&pack).await.unwrap();

    let (st, headers, body) = get(&state, &format!("/api/packs/{}/export", pack.id)).await;
    assert_eq!(st, StatusCode::OK);
    assert!(headers.get(header::CONTENT_TYPE).unwrap().to_str().unwrap().contains("application/json"));
    let v: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(v["format"], "shirita.pack");
}
