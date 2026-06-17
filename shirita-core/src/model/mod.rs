//! 模型适配层：统一的流式聊天接口。

pub mod anthropic;
pub mod echo;
pub mod openai;

use async_trait::async_trait;
use futures::stream::BoxStream;

use crate::models::message::Role;
use crate::Result;

pub use anthropic::AnthropicProvider;
pub use echo::EchoProvider;
pub use openai::OpenAiProvider;

/// 发给模型的单条消息（与持久化的 Message 解耦）。
#[derive(Debug, Clone, PartialEq)]
pub struct ChatMessage {
    pub role: Role,
    pub content: String,
}

/// 一次聊天补全请求。
#[derive(Debug, Clone)]
pub struct ChatRequest {
    pub model: String,
    pub messages: Vec<ChatMessage>,
    /// 当前分支的滚动摘要（若有）。放进请求体哪里由各 provider 决定（见 M6 spec §4）。
    pub summary: Option<String>,
    /// 回复（输出）最大 token 数。非上下文窗口。`None` 时 Anthropic 取内置默认、
    /// OpenAI 省略该字段（用服务端默认）。来源：settings `provider_max_tokens`。
    pub max_tokens: Option<u32>,
}

/// 流式聊天：每个元素是一段文本增量；流结束即 done。
#[async_trait]
pub trait ModelProvider: Send + Sync {
    async fn stream_chat(&self, req: ChatRequest) -> Result<BoxStream<'static, Result<String>>>;
}

/// 解析 OpenAI SSE 中 `data:` 之后的 JSON，提取 `choices[0].delta.content`。
/// 仅含 role（无 content）的首帧返回 `Ok(None)`。
pub fn parse_delta(json_after_data: &str) -> Result<Option<String>> {
    let v: serde_json::Value = serde_json::from_str(json_after_data)?;
    Ok(v["choices"][0]["delta"]["content"]
        .as_str()
        .map(|s| s.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_delta_extracts_content() {
        let line = r#"{"choices":[{"delta":{"content":"He"}}]}"#;
        assert_eq!(parse_delta(line).unwrap(), Some("He".to_string()));
    }

    #[test]
    fn parse_delta_role_only_is_none() {
        let line = r#"{"choices":[{"delta":{"role":"assistant"}}]}"#;
        assert_eq!(parse_delta(line).unwrap(), None);
    }

    #[test]
    fn parse_delta_invalid_json_errors() {
        assert!(parse_delta("not json").is_err());
    }
}
