use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use serde_json::Value;
use futures::StreamExt;

use shirita_core::{ChatMessage, ChatRequest, Role};

use crate::provider_select::{build_provider, models_request, normalize_models_response, resolve_provider_config};
use crate::AppState;

pub async fn test_connection(State(state): State<AppState>) -> Result<Json<Value>, StatusCode> {
    let (source, base_url, api_key, model) = resolve_provider_config(state.storage.as_ref()).await;
    let model = if model.is_empty() { "gpt-4o".to_string() } else { model };
    // A builder that shares the same origin as the actual generation (compatible with anthropic, ollama, and OpenAI), reusing a shared client.
    let provider = build_provider(state.http_client.clone(), &source, &base_url, &api_key);
    let req = ChatRequest { model, messages: vec![ChatMessage { role: Role::User, content: "ping".into(), ..Default::default() }], summary: None, max_tokens: Some(16) };
    match provider.stream_chat(req).await {
        // Only the first streamed chunk matters: it confirms the credentials
        // and endpoint accept a request.
        Ok(mut stream) => match stream.next().await {
            Some(Ok(_)) => Ok(Json(serde_json::json!({ "ok": true }))),
            Some(Err(e)) => Ok(Json(serde_json::json!({ "ok": false, "error": e.to_string() }))),
            None => Ok(Json(serde_json::json!({ "ok": false, "error": "no response from provider" }))),
        },
        Err(e) => Ok(Json(serde_json::json!({ "ok": false, "error": e.to_string() }))),
    }
}

pub async fn list_models(State(state): State<AppState>) -> Result<Json<Value>, StatusCode> {
    let (source, base_url, api_key, _model) = resolve_provider_config(state.storage.as_ref()).await;
    let client = state.http_client.clone();
    // Each vendor has its own auth scheme and response shape for listing
    // models; build the right request and normalize the result so the
    // frontend always sees an OpenAI-style { data: [{ id }] } list.
    let req = models_request(&source, &base_url, &api_key);
    // Bounded, non-streaming request: cap the whole round-trip so a hung or
    // slow provider can't block the handler indefinitely.
    let mut rb = client.get(&req.url).timeout(std::time::Duration::from_secs(30));
    for (k, v) in &req.headers {
        rb = rb.header(*k, v);
    }
    match rb.send().await {
        Ok(resp) => {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            if !status.is_success() {
                return Ok(Json(serde_json::json!({ "error": format!("provider {status}: {text}") })));
            }
            match serde_json::from_str::<Value>(&text) {
                Ok(json) => Ok(Json(normalize_models_response(&source, &json))),
                Err(e) => Ok(Json(serde_json::json!({ "error": format!("invalid response from /models: {e}") }))),
            }
        }
        Err(e) => Ok(Json(serde_json::json!({ "error": e.to_string() }))),
    }
}
