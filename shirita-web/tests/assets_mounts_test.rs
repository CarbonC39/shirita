use std::sync::Arc;

use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use http_body_util::BodyExt;
use tower::ServiceExt;

use shirita_core::{
    Config, EchoProvider, ModelProvider, Session, SqliteStorage, Storage, TiktokenCounter,
    TokenCounter,
};
use shirita_web::{app, AppState};

async fn state_with_assets() -> (AppState, std::path::PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let base = dir.path().to_path_buf();
    std::mem::forget(dir);
    let assets = base.join("assets");
    std::fs::create_dir_all(&assets).unwrap();
    let db = base.join("am.db");
    let storage = SqliteStorage::connect(db.to_str().unwrap()).await.unwrap();
    storage.run_migrations().await.unwrap();
    let storage: Arc<dyn Storage> = Arc::new(storage);
    let config = Arc::new(
        Config::new(db.to_str().unwrap(), assets.to_str().unwrap(), "secret-token").unwrap(),
    );
    let provider: Arc<dyn ModelProvider> = Arc::new(EchoProvider);
    let token_counter: Arc<dyn TokenCounter> = Arc::new(TiktokenCounter::new());
    (
        AppState {
            storage,
            config,
            provider,
            token_counter,
            model: "m".into(),
            generations: Arc::new(shirita_web::Generations::new()),
        },
        assets,
    )
}

#[tokio::test]
async fn upload_writes_file_and_returns_url() {
    let (state, assets) = state_with_assets().await;
    let boundary = "BOUNDARY";
    let body = format!(
        "--{b}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"pic.png\"\r\nContent-Type: application/octet-stream\r\n\r\nHELLO\r\n--{b}--\r\n",
        b = boundary
    );
    let req = Request::builder()
        .method("POST")
        .uri("/api/assets")
        .header(header::AUTHORIZATION, "Bearer secret-token")
        .header(
            header::CONTENT_TYPE,
            format!("multipart/form-data; boundary={boundary}"),
        )
        .body(Body::from(body))
        .unwrap();
    let res = app(state).oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let name = v["path"].as_str().unwrap();
    assert!(v["url"].as_str().unwrap().starts_with("/assets/"));
    assert!(name.ends_with(".png"));
    assert_eq!(std::fs::read(assets.join(name)).unwrap(), b"HELLO");
}

#[tokio::test]
async fn set_and_read_mounts() {
    let (state, _) = state_with_assets().await;
    let s = Session::new("c");
    state.storage.create_session(&s).await.unwrap();
    let req = Request::builder()
        .method("PUT")
        .uri(format!("/api/sessions/{}/mounts", s.id))
        .header(header::AUTHORIZATION, "Bearer secret-token")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(r#"{"definition_ids":["d1","d2"]}"#))
        .unwrap();
    let res = app(state.clone()).oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let got = state.storage.get_session(&s.id).await.unwrap().unwrap();
    assert_eq!(got.mounted_definitions, vec!["d1", "d2"]);
}

#[tokio::test]
async fn mount_unknown_session_404() {
    let (state, _) = state_with_assets().await;
    let req = Request::builder()
        .method("PUT")
        .uri("/api/sessions/ghost/mounts")
        .header(header::AUTHORIZATION, "Bearer secret-token")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(r#"{"definition_ids":["d1"]}"#))
        .unwrap();
    let res = app(state).oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::NOT_FOUND);
}
