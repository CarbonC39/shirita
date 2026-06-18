use std::sync::Arc;

use serde_json::Value;
use shirita_core::{AnthropicProvider, Config, EchoProvider, ModelProvider, OpenAiProvider};

use crate::AppState;

/// provider 选择的纯决策结果（便于单测）。
#[derive(Debug, PartialEq, Eq)]
pub enum ProviderKind {
    Anthropic,
    Ollama,
    OpenAi,
    Echo,
}

/// 由 `PROVIDER` env 值与 api_key 是否为空，决定适配器种类。纯函数。
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

/// 各「source」的默认 base url。anthropic 不带 `/v1`（AnthropicProvider 自行追加 `/v1/messages`）；
/// 其余为 OpenAI 兼容端点（OpenAiProvider 追加 `/chat/completions`）。
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

/// 由 source/base_url/api_key + 共享 client 构造 provider（纯决策）。anthropic 用 Anthropic 适配器，
/// ollama 用占位 key 的 OpenAI 兼容，其余走 OpenAI 兼容。
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

/// 按 env 构造兜底 provider（启动时一次）。复用传入的共享 client。
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

async fn setting_str(state: &AppState, key: &str) -> Option<String> {
    state
        .storage
        .get_setting(key)
        .await
        .ok()
        .flatten()
        .and_then(|v| v.as_str().map(|s| s.to_string()))
}

/// 生成时解析实际 (provider, model)：settings 配置了任一 provider 字段则胜出，
/// 否则整体回退到 env 构造的 `state.provider`/`state.model`（保证桌面首启离线 Echo 不破）。
/// 复用 `state.http_client`，不新建 reqwest::Client。
pub async fn resolve_provider(state: &AppState) -> (Arc<dyn ModelProvider>, String) {
    let source = setting_str(state, "provider_source").await;
    let base_url = setting_str(state, "provider_base_url").await;
    let api_key = setting_str(state, "provider_api_key").await;
    let model = setting_str(state, "provider_model").await;

    let nonempty = |o: &Option<String>| o.as_deref().is_some_and(|s| !s.is_empty());
    if !(nonempty(&source) || nonempty(&api_key) || nonempty(&model)) {
        return (state.provider.clone(), state.model.clone()); // 未配置 → env 兜底
    }

    let source = source.filter(|s| !s.is_empty()).unwrap_or_else(|| "openai".into());
    let base_url = base_url
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| default_base_url(&source).to_string());
    let api_key = api_key.unwrap_or_default();
    let model = model.filter(|s| !s.is_empty()).unwrap_or_else(|| state.model.clone());

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
        // AnthropicProvider 追加 /v1/messages，故 base 不能自带 /v1。
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
