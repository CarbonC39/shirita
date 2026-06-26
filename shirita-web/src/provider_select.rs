use std::sync::Arc;

use serde_json::Value;
use shirita_core::{AnthropicProvider, Config, EchoProvider, ModelProvider, OpenAiProvider};

use crate::AppState;

/// The pure decision result selected by the provider (for unit testing purposes).
#[derive(Debug, PartialEq, Eq)]
pub enum ProviderKind {
    Anthropic,
    Ollama,
    OpenAi,
    Echo,
}

/// The adapter type is determined by the `PROVIDER` environment variable and whether the `api_key` is null. Pure function.
pub fn provider_kind(provider_env: &str, api_key_empty: bool) -> ProviderKind {
    match provider_env {
        "anthropic" => ProviderKind::Anthropic,
        "ollama" => ProviderKind::Ollama,
        _ => {
            if api_key_empty {
                ProviderKind::Echo
            } else {
                ProviderKind::OpenAi
            }
        }
    }
}

/// The default base URL for each “source.” Anthropic does not include `/v1` (AnthropicProvider automatically appends `/v1/messages`);
/// The rest are OpenAI-compatible endpoints (OpenAiProvider appends `/chat/completions`).
pub(crate) fn default_base_url(source: &str) -> &'static str {
    match source {
        "openai" => "https://api.openai.com/v1",
        "anthropic" => "https://api.anthropic.com",
        "ollama" => "http://localhost:11434/v1",
        "google" => "https://generativelanguage.googleapis.com/v1beta",
        "openrouter" => "https://openrouter.ai/api/v1",
        "mistral" => "https://api.mistral.ai/v1",
        "deepseek" => "https://api.deepseek.com/v1",
        "groq" => "https://api.groq.com/openai/v1",
        "xai" => "https://api.x.ai/v1",
        "cohere" => "https://api.cohere.ai/v1",
        "together" => "https://api.together.xyz/v1",
        "perplexity" => "https://api.perplexity.ai",
        _ => "https://api.openai.com/v1",
    }
}

/// Construct a provider using source/base_url/api_key + the shared client (pure decision-making). For Anthropic, use the Anthropic adapter;
/// for Ollama, use the OpenAI-compatible adapter with a placeholder key; for all others, use the OpenAI-compatible adapter.
pub(crate) fn build_provider(
    client: reqwest::Client,
    source: &str,
    base_url: &str,
    api_key: &str,
) -> Arc<dyn ModelProvider> {
    match source {
        "anthropic" => Arc::new(AnthropicProvider::new(client, base_url, api_key)),
        "ollama" => Arc::new(OpenAiProvider::new(client, base_url, "ollama")),
        _ => Arc::new(OpenAiProvider::new(client, base_url, api_key)),
    }
}

/// Construct a fallback provider based on `env` (once at startup). Reuse the shared client passed in.
pub fn provider_from_env(config: &Config, client: reqwest::Client) -> Arc<dyn ModelProvider> {
    let provider_env = std::env::var("PROVIDER").unwrap_or_default();
    match provider_kind(&provider_env, config.openai_api_key.is_empty()) {
        ProviderKind::Anthropic => {
            let base = std::env::var("ANTHROPIC_BASE_URL")
                .unwrap_or_else(|_| "https://api.anthropic.com".into());
            tracing::info!("using AnthropicProvider at {base}");
            Arc::new(AnthropicProvider::new(client, base, config.openai_api_key.clone()))
        }
        ProviderKind::Ollama => {
            let base = std::env::var("OLLAMA_BASE_URL")
                .unwrap_or_else(|_| "http://localhost:11434/v1".into());
            tracing::info!("using Ollama (OpenAI-compatible) at {base}");
            Arc::new(OpenAiProvider::new(client, base, "ollama"))
        }
        ProviderKind::OpenAi => {
            tracing::info!("using OpenAiProvider at {}", config.openai_base_url);
            Arc::new(OpenAiProvider::new(
                client,
                config.openai_base_url.clone(),
                config.openai_api_key.clone(),
            ))
        }
        ProviderKind::Echo => {
            tracing::info!("OPENAI_API_KEY empty: using offline EchoProvider");
            Arc::new(EchoProvider)
        }
    }
}

/// `GET /models` request shape for a given source: url + extra headers. Each
/// vendor authenticates and addresses its model-listing endpoint differently
/// (Anthropic wants `x-api-key`, Google wants the key as a query param), so
/// this is kept as a pure, unit-testable decision separate from the network call.
pub(crate) struct ModelsRequest {
    pub url: String,
    pub headers: Vec<(&'static str, String)>,
}

pub(crate) fn models_request(source: &str, base_url: &str, api_key: &str) -> ModelsRequest {
    let base = base_url.trim_end_matches('/');
    match source {
        "anthropic" => ModelsRequest {
            url: format!("{base}/v1/models"),
            headers: vec![
                ("x-api-key", api_key.to_string()),
                ("anthropic-version", "2023-06-01".to_string()),
            ],
        },
        "google" => ModelsRequest {
            url: format!("{base}/models?key={api_key}"),
            headers: vec![],
        },
        "ollama" => ModelsRequest { url: format!("{base}/models"), headers: vec![] },
        _ => ModelsRequest {
            url: format!("{base}/models"),
            headers: vec![("Authorization", format!("Bearer {api_key}"))],
        },
    }
}

/// Normalize a vendor's raw `/models` response into the OpenAI-style
/// `{ "data": [{ "id": ... }] }` envelope the frontend always expects.
/// OpenAI-compatible sources and Anthropic already return this shape;
/// Google and Cohere nest models under a different key/field and need mapping.
pub(crate) fn normalize_models_response(source: &str, raw: &Value) -> Value {
    match source {
        "google" => {
            let ids: Vec<Value> = raw
                .get("models")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|m| m.get("name").and_then(|n| n.as_str()))
                        .map(|n| serde_json::json!({ "id": n.rsplit('/').next().unwrap_or(n) }))
                        .collect()
                })
                .unwrap_or_default();
            serde_json::json!({ "data": ids })
        }
        "cohere" => {
            let ids: Vec<Value> = raw
                .get("models")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|m| m.get("name").and_then(|n| n.as_str()))
                        .map(|n| serde_json::json!({ "id": n }))
                        .collect()
                })
                .unwrap_or_default();
            serde_json::json!({ "data": ids })
        }
        _ => raw.clone(),
    }
}

async fn setting_str_storage(storage: &dyn shirita_core::Storage, key: &str) -> Option<String> {
    storage.get_setting(key).await.ok().flatten().and_then(|v| v.as_str().map(|s| s.to_string()))
}

/// Resolve the active provider's per-source config from namespaced settings,
/// migrating legacy flat `provider_*` keys into the active source's namespace
/// on first use. Returns (source, base_url, api_key, model); base_url falls back
/// to the source default, api_key/model are empty when unset (callers apply
/// their own model default / env fallback).
pub async fn resolve_provider_config(
    storage: &dyn shirita_core::Storage,
) -> (String, String, String, String) {
    let source = setting_str_storage(storage, "provider_source")
        .await
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "openai".into());

    // Migrate legacy flat keys once: if the namespaced key is unset but a flat
    // one exists, copy it over (the flat key is left as a harmless remnant).
    for (flat, field) in [
        ("provider_base_url", "base_url"),
        ("provider_api_key", "api_key"),
        ("provider_model", "model"),
    ] {
        let ns = format!("provider.{source}.{field}");
        if setting_str_storage(storage, &ns).await.is_none() {
            if let Some(v) = setting_str_storage(storage, flat).await {
                let _ = storage.set_setting(&ns, &serde_json::json!(v)).await;
            }
        }
    }

    let base_url = setting_str_storage(storage, &format!("provider.{source}.base_url"))
        .await
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| default_base_url(&source).to_string());
    let api_key = setting_str_storage(storage, &format!("provider.{source}.api_key"))
        .await
        .unwrap_or_default();
    let model = setting_str_storage(storage, &format!("provider.{source}.model"))
        .await
        .unwrap_or_default();
    (source, base_url, api_key, model)
}

/// Parse the actual (provider, model) during initialization: if any provider field is configured in settings, it takes precedence;
/// otherwise, fall back to `state.provider`/`state.model` as constructed from env (to ensure Echo remains functional offline during the first desktop launch).
/// Reuse `state.http_client`; do not create a new `request::Client`.
pub async fn resolve_provider(state: &AppState) -> (Arc<dyn ModelProvider>, String) {
    let storage = state.storage.as_ref();
    // "Configured" = the user explicitly set the active source, or has a key/model
    // for it (flat legacy keys are migrated into the namespace by the resolver).
    let source_set = setting_str_storage(storage, "provider_source").await;
    let (source, base_url, api_key, model) = resolve_provider_config(storage).await;

    let nonempty = |s: &str| !s.is_empty();
    let configured = source_set.as_deref().is_some_and(nonempty)
        || nonempty(&api_key)
        || nonempty(&model);
    if !configured {
        return (state.provider.clone(), state.model.clone()); // → env fallback
    }

    let model = if model.is_empty() { state.model.clone() } else { model };
    let provider = build_provider(state.http_client.clone(), &source, &base_url, &api_key);
    (provider, model)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_kind_maps_env_and_key() {
        assert_eq!(provider_kind("anthropic", false), ProviderKind::Anthropic);
        assert_eq!(provider_kind("ollama", true), ProviderKind::Ollama);
        assert_eq!(provider_kind("", false), ProviderKind::OpenAi);
        assert_eq!(provider_kind("", true), ProviderKind::Echo);
        assert_eq!(provider_kind("unknown", true), ProviderKind::Echo);
    }

    #[test]
    fn default_base_url_anthropic_has_no_v1_suffix() {
        // AnthropicProvider appends /v1/messages, so the base cannot include /v1.
        assert_eq!(default_base_url("anthropic"), "https://api.anthropic.com");
        assert_eq!(default_base_url("openai"), "https://api.openai.com/v1");
        assert_eq!(default_base_url("nonsense"), "https://api.openai.com/v1");
    }

    #[test]
    fn default_base_url_ollama_points_at_local_daemon() {
        assert_eq!(default_base_url("ollama"), "http://localhost:11434/v1");
    }

    #[test]
    fn models_request_ollama_sends_no_auth_header() {
        let req = models_request("ollama", "http://localhost:11434/v1", "");
        assert_eq!(req.url, "http://localhost:11434/v1/models");
        assert!(req.headers.is_empty());
    }

    #[test]
    fn models_request_anthropic_uses_x_api_key_header() {
        let req = models_request("anthropic", "https://api.anthropic.com", "sk-ant-1");
        assert_eq!(req.url, "https://api.anthropic.com/v1/models");
        assert!(req.headers.contains(&("x-api-key", "sk-ant-1".to_string())));
        assert!(req.headers.contains(&("anthropic-version", "2023-06-01".to_string())));
    }

    #[test]
    fn models_request_google_uses_key_query_param_not_header() {
        let req = models_request("google", "https://generativelanguage.googleapis.com/v1beta", "AIza-key");
        assert_eq!(req.url, "https://generativelanguage.googleapis.com/v1beta/models?key=AIza-key");
        assert!(req.headers.is_empty());
    }

    #[test]
    fn models_request_openai_compatible_uses_bearer_header() {
        let req = models_request("openai", "https://api.openai.com/v1", "sk-1");
        assert_eq!(req.url, "https://api.openai.com/v1/models");
        assert_eq!(req.headers, vec![("Authorization", "Bearer sk-1".to_string())]);
    }

    #[test]
    fn normalize_models_response_passes_through_openai_shape() {
        let raw = serde_json::json!({ "data": [{ "id": "gpt-4o" }] });
        assert_eq!(normalize_models_response("openai", &raw), raw);
    }

    #[test]
    fn normalize_models_response_passes_through_anthropic_shape() {
        // Anthropic's /v1/models already returns { data: [{ id, ... }] }.
        let raw = serde_json::json!({ "data": [{ "id": "claude-opus-4-8" }] });
        assert_eq!(normalize_models_response("anthropic", &raw), raw);
    }

    #[test]
    fn normalize_models_response_extracts_google_model_ids() {
        let raw = serde_json::json!({ "models": [
            { "name": "models/gemini-2.5-pro" },
            { "name": "models/gemini-2.5-flash" },
        ] });
        let got = normalize_models_response("google", &raw);
        assert_eq!(
            got,
            serde_json::json!({ "data": [{ "id": "gemini-2.5-pro" }, { "id": "gemini-2.5-flash" }] })
        );
    }

    #[test]
    fn normalize_models_response_extracts_cohere_model_ids() {
        let raw = serde_json::json!({ "models": [{ "name": "command-r-plus" }] });
        let got = normalize_models_response("cohere", &raw);
        assert_eq!(got, serde_json::json!({ "data": [{ "id": "command-r-plus" }] }));
    }
}
