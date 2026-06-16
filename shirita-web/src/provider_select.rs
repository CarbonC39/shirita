use std::sync::Arc;

use shirita_core::{AnthropicProvider, Config, EchoProvider, ModelProvider, OpenAiProvider};

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

/// 按 env 构造 provider（移自 main.rs）。读取 PROVIDER 及对应 base url env。
pub fn provider_from_env(config: &Config) -> Arc<dyn ModelProvider> {
    let provider_env = std::env::var("PROVIDER").unwrap_or_default();
    match provider_kind(&provider_env, config.openai_api_key.is_empty()) {
        ProviderKind::Anthropic => {
            let base = std::env::var("ANTHROPIC_BASE_URL")
                .unwrap_or_else(|_| "https://api.anthropic.com".into());
            tracing::info!("using AnthropicProvider at {base}");
            Arc::new(AnthropicProvider::new(base, config.openai_api_key.clone()))
        }
        ProviderKind::Ollama => {
            let base = std::env::var("OLLAMA_BASE_URL")
                .unwrap_or_else(|_| "http://localhost:11434/v1".into());
            tracing::info!("using Ollama (OpenAI-compatible) at {base}");
            Arc::new(OpenAiProvider::new(base, "ollama"))
        }
        ProviderKind::OpenAi => {
            tracing::info!("using OpenAiProvider at {}", config.openai_base_url);
            Arc::new(OpenAiProvider::new(
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
}
