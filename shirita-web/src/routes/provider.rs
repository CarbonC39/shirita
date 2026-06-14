use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use serde_json::Value;
use futures::StreamExt;

use shirita_core::{ChatMessage, ChatRequest, ModelProvider, OpenAiProvider, Role};

use crate::AppState;

pub async fn test_connection(State(state): State<AppState>) -> Result<Json<Value>, StatusCode> {
    let source = state.storage.get_setting("provider_source").await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?.and_then(|v| v.as_str().map(|s| s.to_string())).unwrap_or_else(|| "openai".into());
    let base_url = state.storage.get_setting("provider_base_url").await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?.and_then(|v| v.as_str().map(|s| s.to_string())).unwrap_or_else(|| default_base_url(&source).into());
    let api_key = state.storage.get_setting("provider_api_key").await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?.and_then(|v| v.as_str().map(|s| s.to_string())).unwrap_or_default();
    let model = state.storage.get_setting("provider_model").await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?.and_then(|v| v.as_str().map(|s| s.to_string())).unwrap_or_else(|| "gpt-4o".into());
    let provider = OpenAiProvider::new(&base_url, &api_key);
    let req = ChatRequest { model, messages: vec![ChatMessage { role: Role::User, content: "ping".into() }] };
    match provider.stream_chat(req).await {
        Ok(mut stream) => {
            let mut received = false;
            while let Some(item) = stream.next().await {
                match item { Ok(_) => { received = true; break; } Err(e) => return Ok(Json(serde_json::json!({ "ok": false, "error": e.to_string() }))) }
            }
            Ok(Json(serde_json::json!({ "ok": received || true })))
        }
        Err(e) => Ok(Json(serde_json::json!({ "ok": false, "error": e.to_string() }))),
    }
}

pub async fn list_models(State(state): State<AppState>) -> Result<Json<Value>, StatusCode> {
    let source = state.storage.get_setting("provider_source").await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?.and_then(|v| v.as_str().map(|s| s.to_string())).unwrap_or_else(|| "openai".into());
    let base_url = state.storage.get_setting("provider_base_url").await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?.and_then(|v| v.as_str().map(|s| s.to_string())).unwrap_or_else(|| default_base_url(&source).into());
    let api_key = state.storage.get_setting("provider_api_key").await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?.and_then(|v| v.as_str().map(|s| s.to_string())).unwrap_or_default();
    let client = reqwest::Client::new();
    let url = format!("{}/models", base_url.trim_end_matches('/'));
    match client.get(&url).header("Authorization", format!("Bearer {}", api_key)).send().await {
        Ok(resp) => {
            let json: Value = resp.json().await.unwrap_or_default();
            Ok(Json(json))
        }
        Err(e) => Ok(Json(serde_json::json!({ "error": e.to_string() }))),
    }
}

fn default_base_url(source: &str) -> &str {
    match source {
        "openai" => "https://api.openai.com/v1", "anthropic" => "https://api.anthropic.com/v1",
        "google" => "https://generativelanguage.googleapis.com/v1beta", "openrouter" => "https://openrouter.ai/api/v1",
        "mistral" => "https://api.mistral.ai/v1", "deepseek" => "https://api.deepseek.com/v1",
        "groq" => "https://api.groq.com/openai/v1", "xai" => "https://api.x.ai/v1",
        "cohere" => "https://api.cohere.ai/v1", "together" => "https://api.together.xyz/v1",
        "perplexity" => "https://api.perplexity.ai", _ => "https://api.openai.com/v1",
    }
}
