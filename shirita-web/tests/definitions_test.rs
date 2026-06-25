use std::sync::Arc;

use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use http_body_util::BodyExt;
use tower::ServiceExt;

use shirita_core::{
    Config, EchoProvider, ModelProvider, SqliteStorage, Storage, TiktokenCounter, TokenCounter,
};
use shirita_web::{app, AppState};

async fn test_state() -> AppState {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("def_test.db");
    std::mem::forget(dir);
    let storage = SqliteStorage::connect(path.to_str().unwrap()).await.unwrap();
    storage.run_migrations().await.unwrap();
    let storage: Arc<dyn Storage> = Arc::new(storage);
    let config = Arc::new(Config::new("ignored", "./assets", "secret-token").unwrap());
    let provider: Arc<dyn ModelProvider> = Arc::new(EchoProvider);
    let token_counter: Arc<dyn TokenCounter> = Arc::new(TiktokenCounter::new());
    AppState {
        storage,
        config,
        provider,
        token_counter,
        model: "m".into(),
        generations: Arc::new(shirita_web::Generations::new()), http_client: shirita_web::new_http_client(),
    }
}

fn req(method: &str, uri: &str, body: Option<&str>) -> Request<Body> {
    let mut b = Request::builder()
        .method(method)
        .uri(uri)
        .header(header::AUTHORIZATION, "Bearer secret-token");
    if body.is_some() {
        b = b.header(header::CONTENT_TYPE, "application/json");
    }
    b.body(body.map(|s| Body::from(s.to_string())).unwrap_or(Body::empty()))
        .unwrap()
}

#[tokio::test]
async fn definition_crud_over_http() {
    let state = test_state().await;

    // create
    let res = app(state.clone())
        .oneshot(req(
            "POST",
            "/api/definitions",
            Some(r#"{"type":"char","name":"Alice","content":"<c/>"}"#),
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = res.into_body().collect().await.unwrap().to_bytes();
    let created: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let id = created["id"].as_str().unwrap().to_string();
    assert_eq!(created["type"], "char");

    // get
    let res = app(state.clone())
        .oneshot(req("GET", &format!("/api/definitions/{id}"), None))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    // list with type filter
    let res = app(state.clone())
        .oneshot(req("GET", "/api/definitions?type=char", None))
        .await
        .unwrap();
    let body = res.into_body().collect().await.unwrap().to_bytes();
    let list: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(list.as_array().unwrap().len(), 1);
    let res = app(state.clone())
        .oneshot(req("GET", "/api/definitions?type=world", None))
        .await
        .unwrap();
    let body = res.into_body().collect().await.unwrap().to_bytes();
    let list: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(list.as_array().unwrap().len(), 0);

    // update
    let res = app(state.clone())
        .oneshot(req(
            "PUT",
            &format!("/api/definitions/{id}"),
            Some(r#"{"type":"persona","name":"Al","content":"x"}"#),
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = res.into_body().collect().await.unwrap().to_bytes();
    let updated: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(updated["type"], "persona");

    // delete
    let res = app(state.clone())
        .oneshot(req("DELETE", &format!("/api/definitions/{id}"), None))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::NO_CONTENT);
    let res = app(state)
        .oneshot(req("GET", &format!("/api/definitions/{id}"), None))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn update_unknown_definition_404() {
    let res = app(test_state().await)
        .oneshot(req(
            "PUT",
            "/api/definitions/ghost",
            Some(r#"{"type":"char","name":"x","content":"y"}"#),
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn bad_type_is_rejected() {
    let res = app(test_state().await)
        .oneshot(req(
            "POST",
            "/api/definitions",
            Some(r#"{"type":"nope","name":"x","content":"y"}"#),
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn regex_rule_with_bad_pattern_is_rejected() {
    let state = test_state().await;

    // 非法 pattern（未闭合的字符类）→ 400。
    let res = app(state.clone())
        .oneshot(req(
            "POST",
            "/api/definitions",
            Some(r#"{"type":"regex_rule","name":"r","content":"","meta":{"pattern":"["}}"#),
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);

    // 合法 pattern → 200。
    let res = app(state.clone())
        .oneshot(req(
            "POST",
            "/api/definitions",
            Some(r#"{"type":"regex_rule","name":"r","content":"","meta":{"pattern":"\\d+"}}"#),
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = res.into_body().collect().await.unwrap().to_bytes();
    let created: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let id = created["id"].as_str().unwrap().to_string();

    // 空 pattern 宽容放行（创作中途）→ 200。
    let res = app(state.clone())
        .oneshot(req(
            "POST",
            "/api/definitions",
            Some(r#"{"type":"regex_rule","name":"r2","content":"","meta":{"pattern":""}}"#),
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    // 更新为非法 pattern 同样 400。
    let res = app(state)
        .oneshot(req(
            "PUT",
            &format!("/api/definitions/{id}"),
            Some(r#"{"type":"regex_rule","name":"r","content":"","meta":{"pattern":"(unclosed"}}"#),
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn updating_a_missing_definition_is_404() {
    let state = test_state().await;
    let res = app(state.clone())
        .oneshot(req(
            "PUT",
            "/api/definitions/does-not-exist",
            Some(r#"{"type":"char","name":"X","content":"y"}"#),
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn custom_container_type_is_accepted_after_registration() {
    let state = test_state().await;
    // register a custom container type
    let res = app(state.clone())
        .oneshot(req(
            "POST",
            "/api/types",
            Some(r#"{"id":"faction","label":"Faction"}"#),
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    // a definition of that type is now valid
    let res = app(state.clone())
        .oneshot(req(
            "POST",
            "/api/definitions",
            Some(r#"{"type":"faction","name":"Zion","content":"x"}"#),
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    // an unregistered type is still rejected
    let res = app(state.clone())
        .oneshot(req(
            "POST",
            "/api/definitions",
            Some(r#"{"type":"bogus","name":"X","content":"x"}"#),
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
}
