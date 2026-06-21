//! POST /api/import — shirita.pack zip bundles: round-trip, dedup, safety, degraded JSON.

use std::sync::Arc;

use axum::body::Body;
use axum::http::{header, HeaderMap, Request, StatusCode};
use http_body_util::BodyExt;
use serde_json::Value;
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

async fn export_zip(state: &AppState, pack_id: &str) -> (HeaderMap, Vec<u8>) {
    let req = Request::builder()
        .method("GET")
        .uri(format!("/api/packs/{pack_id}/export"))
        .header(header::AUTHORIZATION, "Bearer secret-token")
        .body(Body::empty())
        .unwrap();
    let res = app(state.clone()).oneshot(req).await.unwrap();
    let headers = res.headers().clone();
    (headers, res.into_body().collect().await.unwrap().to_bytes().to_vec())
}

async fn import_bytes(state: &AppState, query: &str, data: &[u8]) -> (StatusCode, Value) {
    let boundary = "BND";
    let mut body: Vec<u8> = Vec::new();
    body.extend_from_slice(format!(
        "--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"p.bin\"\r\nContent-Type: application/octet-stream\r\n\r\n"
    ).as_bytes());
    body.extend_from_slice(data);
    body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
    let req = Request::builder()
        .method("POST")
        .uri(format!("/api/import{query}"))
        .header(header::AUTHORIZATION, "Bearer secret-token")
        .header(header::CONTENT_TYPE, format!("multipart/form-data; boundary={boundary}"))
        .body(Body::from(body))
        .unwrap();
    let res = app(state.clone()).oneshot(req).await.unwrap();
    let status = res.status();
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    (status, serde_json::from_slice(&bytes).unwrap_or(Value::Null))
}

/// Build an in-memory zip: manifest.json + the given (entry-name, bytes) pairs.
fn make_zip(manifest: &Value, entries: &[(&str, &[u8])]) -> Vec<u8> {
    use std::io::Write;
    let mut cur = std::io::Cursor::new(Vec::new());
    {
        let mut zw = zip::ZipWriter::new(&mut cur);
        let opts = zip::write::SimpleFileOptions::default();
        zw.start_file("manifest.json", opts).unwrap();
        zw.write_all(&serde_json::to_vec(manifest).unwrap()).unwrap();
        for (name, data) in entries {
            zw.start_file(*name, opts).unwrap();
            zw.write_all(data).unwrap();
        }
        zw.finish().unwrap();
    }
    cur.into_inner()
}

fn created_pack_id(summary: &Value) -> String {
    summary["created"].as_array().unwrap().iter()
        .find(|c| c["kind"] == "pack").expect("a pack was created")["id"].as_str().unwrap().to_string()
}

#[tokio::test]
async fn import_pack_zip_round_trips_and_restores_asset() {
    // Source instance: a pack with an avatar → export a zip.
    let src = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(src.path().join("assets")).unwrap();
    std::fs::write(src.path().join("assets/face.png"), b"img-bytes").unwrap();
    let s1 = test_state(src.path()).await;
    let mut pack = Pack::new("Alice");
    pack.identity.avatar = Some("face.png".into());
    s1.storage.create_pack(&pack).await.unwrap();
    let (_, zip_bytes) = export_zip(&s1, &pack.id).await;
    assert_eq!(&zip_bytes[..2], b"PK");

    // Destination instance: import the zip plug-and-play.
    let dst = tempfile::tempdir().unwrap();
    let s2 = test_state(dst.path()).await;
    let (st, summary) = import_bytes(&s2, "", &zip_bytes).await;
    assert_eq!(st, StatusCode::OK);
    let new_id = created_pack_id(&summary);

    // The imported pack's avatar resolves to a real, restored file with the same bytes.
    let got = s2.storage.get_pack(&new_id).await.unwrap().unwrap();
    let avatar = got.identity.avatar.expect("avatar present after import");
    let restored = dst.path().join("assets").join(&avatar);
    assert!(restored.exists(), "avatar file restored");
    assert_eq!(std::fs::read(&restored).unwrap(), b"img-bytes");
}

#[tokio::test]
async fn import_pack_zip_dedups_asset_by_hash() {
    let src = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(src.path().join("assets")).unwrap();
    std::fs::write(src.path().join("assets/face.png"), b"same-bytes").unwrap();
    let s1 = test_state(src.path()).await;
    let mut pack = Pack::new("Alice");
    pack.identity.avatar = Some("face.png".into());
    s1.storage.create_pack(&pack).await.unwrap();
    let (_, zip_bytes) = export_zip(&s1, &pack.id).await;

    // Import the same bundle twice (duplicate so the 2nd isn't skipped by name).
    let dst = tempfile::tempdir().unwrap();
    let s2 = test_state(dst.path()).await;
    let (_, a) = import_bytes(&s2, "?on_conflict=duplicate", &zip_bytes).await;
    let (_, b) = import_bytes(&s2, "?on_conflict=duplicate", &zip_bytes).await;
    let pa = s2.storage.get_pack(&created_pack_id(&a)).await.unwrap().unwrap();
    let pb = s2.storage.get_pack(&created_pack_id(&b)).await.unwrap().unwrap();
    // Same content hash → both packs point at the SAME stored file (deduped).
    assert_eq!(pa.identity.avatar, pb.identity.avatar);
    let assets = s2.storage.list_assets(None).await.unwrap();
    assert_eq!(assets.len(), 1, "asset registered once, reused on the 2nd import");
}

#[tokio::test]
async fn import_pack_zip_rejects_nested_asset_entry() {
    let dir = tempfile::tempdir().unwrap();
    let state = test_state(dir.path()).await;
    let manifest = serde_json::json!({
        "format": "shirita.pack",
        "pack": { "name": "Evil", "identity": {}, "meta": {} },
        "nodes": [], "definitions": []
    });
    let zip = make_zip(&manifest, &[("assets/sub/evil.png", b"x")]);
    let (st, _) = import_bytes(&state, "", &zip).await;
    assert_eq!(st, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn import_pack_zip_rejects_missing_manifest() {
    let dir = tempfile::tempdir().unwrap();
    let state = test_state(dir.path()).await;
    // A zip with only an asset, no manifest.json.
    use std::io::Write;
    let mut cur = std::io::Cursor::new(Vec::new());
    {
        let mut zw = zip::ZipWriter::new(&mut cur);
        zw.start_file("assets/face.png", zip::write::SimpleFileOptions::default()).unwrap();
        zw.write_all(b"img").unwrap();
        zw.finish().unwrap();
    }
    let (st, _) = import_bytes(&state, "", &cur.into_inner()).await;
    assert_eq!(st, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn import_pack_binary_less_json() {
    let dir = tempfile::tempdir().unwrap();
    let state = test_state(dir.path()).await;
    let manifest = serde_json::json!({
        "format": "shirita.pack",
        "pack": { "name": "NoBin", "identity": {}, "meta": {} },
        "nodes": [], "definitions": []
    });
    let (st, summary) = import_bytes(&state, "", &serde_json::to_vec(&manifest).unwrap()).await;
    assert_eq!(st, StatusCode::OK);
    assert!(summary["created"].as_array().unwrap().iter().any(|c| c["kind"] == "pack"));
    assert!(state.storage.list_packs().await.unwrap().iter().any(|p| p.name == "NoBin"));
}
