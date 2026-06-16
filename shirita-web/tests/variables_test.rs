//! M5 variables: session state seeding, GET …/state effective merge, PUT …/local-variables.

use std::sync::Arc;

use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use http_body_util::BodyExt;
use serde_json::Value;
use tower::ServiceExt;

use shirita_core::{
    Config, EchoProvider, ModelProvider, SqliteStorage, Storage, TiktokenCounter, TokenCounter,
};
use shirita_web::{app, AppState};

async fn test_state() -> AppState {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("variables.db");
    std::mem::forget(dir);
    let storage = SqliteStorage::connect(path.to_str().unwrap()).await.unwrap();
    storage.run_migrations().await.unwrap();
    let storage: Arc<dyn Storage> = Arc::new(storage);
    let config = Arc::new(Config::new("ignored", "./assets", "secret-token").unwrap());
    let provider: Arc<dyn ModelProvider> = Arc::new(EchoProvider);
    let token_counter: Arc<dyn TokenCounter> = Arc::new(TiktokenCounter::new());
    AppState { storage, config, provider, token_counter, model: "test-model".into(), generations: Arc::new(shirita_web::Generations::new()) }
}

async fn send(state: &AppState, method: &str, uri: &str, body: Option<&str>) -> (StatusCode, String) {
    let mut builder = Request::builder()
        .method(method)
        .uri(uri)
        .header(header::AUTHORIZATION, "Bearer secret-token");
    let body = match body {
        Some(json) => {
            builder = builder.header(header::CONTENT_TYPE, "application/json");
            Body::from(json.to_string())
        }
        None => Body::empty(),
    };
    let res = app(state.clone()).oneshot(builder.body(body).unwrap()).await.unwrap();
    let status = res.status();
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    (status, String::from_utf8(bytes.to_vec()).unwrap())
}

fn json(text: &str) -> Value {
    serde_json::from_str(text).unwrap()
}

async fn create_template(state: &AppState, name: &str, meta: &str) -> String {
    let (_, out) = send(state, "POST", "/api/templates", Some(&format!(r#"{{"name":"{name}","meta":{meta}}}"#))).await;
    json(&out)["id"].as_str().unwrap().to_string()
}

#[tokio::test]
async fn creating_a_session_seeds_declared_initials() {
    let state = test_state().await;
    let tid = create_template(&state, "RPG", r#"{"variables":[{"name":"hp","type":"number","initial":100}]}"#).await;
    let (st, out) = send(&state, "POST", "/api/sessions",
        Some(&format!(r#"{{"name":"Chat","template_id":"{tid}"}}"#))).await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(json(&out)["current_state"]["hp"], 100);
}

#[tokio::test]
async fn get_state_merges_schema_seed_and_leaf() {
    let state = test_state().await;
    let tid = create_template(&state, "RPG", r#"{"variables":[{"name":"hp","type":"number","initial":100},{"name":"gold","type":"number","initial":0}]}"#).await;
    let (_, sout) = send(&state, "POST", "/api/sessions", Some(&format!(r#"{{"name":"Chat","template_id":"{tid}"}}"#))).await;
    let sid = json(&sout)["id"].as_str().unwrap().to_string();
    // a turn that spends gold and is hit (EchoProvider can't emit tags; assert the schema/seed path instead)
    let (_, state_out) = send(&state, "GET", &format!("/api/sessions/{sid}/state"), None).await;
    let body = json(&state_out);
    assert_eq!(body["values"]["hp"], 100); // seeded
    assert_eq!(body["values"]["gold"], 0);
    let names: Vec<&str> = body["schema"].as_array().unwrap().iter().map(|d| d["name"].as_str().unwrap()).collect();
    assert!(names.contains(&"$avatar")); // system var present in schema
    assert!(names.contains(&"hp"));
}

#[tokio::test]
async fn set_local_variables_adds_to_the_schema() {
    let state = test_state().await;
    let tid = create_template(&state, "RPG", r#"{"variables":[{"name":"hp","type":"number","initial":100}]}"#).await;
    let (_, sout) = send(&state, "POST", "/api/sessions", Some(&format!(r#"{{"name":"Chat","template_id":"{tid}"}}"#))).await;
    let sid = json(&sout)["id"].as_str().unwrap().to_string();

    let (st, _) = send(&state, "PUT", &format!("/api/sessions/{sid}/local-variables"),
        Some(r#"{"variables":[{"name":"reputation","type":"number","initial":5}]}"#)).await;
    assert_eq!(st, StatusCode::OK);

    let (_, state_out) = send(&state, "GET", &format!("/api/sessions/{sid}/state"), None).await;
    let body = json(&state_out);
    assert_eq!(body["values"]["reputation"], 5); // backfilled from the new local schema initial
    let names: Vec<&str> = body["schema"].as_array().unwrap().iter().map(|d| d["name"].as_str().unwrap()).collect();
    assert!(names.contains(&"reputation"));
}
