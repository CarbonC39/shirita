//! Anthropic Messages API 流式适配器（POST /v1/messages, stream=true）。

use async_trait::async_trait;
use futures::stream::BoxStream;
use futures::StreamExt;
use serde_json::json;

use crate::models::message::Role;
use crate::{Error, Result};

use super::{ChatRequest, ModelProvider};

pub struct AnthropicProvider {
    client: reqwest::Client,
    base_url: String,
    api_key: String,
}

impl AnthropicProvider {
    pub fn new(base_url: impl Into<String>, api_key: impl Into<String>) -> Self {
        Self { client: reqwest::Client::new(), base_url: base_url.into(), api_key: api_key.into() }
    }
}

/// 构造 Anthropic 请求体：System 段合并进顶层 `system`；`summary` 包成 `<history_summary>`
/// **prepend 到第一条 user 消息开头**（保持 user/assistant 交替，避免连续同 role）；无 user 则作首条 user。
pub fn anthropic_body(req: &ChatRequest) -> serde_json::Value {
    let mut system = String::new();
    let mut messages: Vec<serde_json::Value> = Vec::new();
    for m in &req.messages {
        match m.role {
            Role::System => {
                if !system.is_empty() {
                    system.push_str("\n\n");
                }
                system.push_str(&m.content);
            }
            Role::User => messages.push(json!({ "role": "user", "content": m.content })),
            Role::Assistant => messages.push(json!({ "role": "assistant", "content": m.content })),
        }
    }
    if let Some(sum) = &req.summary {
        let wrapped = format!("<history_summary>\n{sum}\n</history_summary>");
        if let Some(first_user) = messages.iter_mut().find(|m| m["role"] == "user") {
            let cur = first_user["content"].as_str().unwrap_or("").to_string();
            first_user["content"] = json!(format!("{wrapped}\n\n{cur}"));
        } else {
            messages.insert(0, json!({ "role": "user", "content": wrapped }));
        }
    }
    json!({
        "model": req.model,
        "stream": true,
        "max_tokens": 4096,
        "system": system,
        "messages": messages,
    })
}

/// 解析 Anthropic SSE `data:` 之后的 JSON：仅 `content_block_delta` 取 `delta.text`，其余返回 None。
pub fn parse_anthropic_delta(json_after_data: &str) -> Result<Option<String>> {
    let v: serde_json::Value = serde_json::from_str(json_after_data)?;
    if v["type"] == "content_block_delta" {
        Ok(v["delta"]["text"].as_str().map(|s| s.to_string()))
    } else {
        Ok(None)
    }
}

#[async_trait]
impl ModelProvider for AnthropicProvider {
    async fn stream_chat(&self, req: ChatRequest) -> Result<BoxStream<'static, Result<String>>> {
        let url = format!("{}/v1/messages", self.base_url.trim_end_matches('/'));
        let body = anthropic_body(&req);

        let resp = self
            .client
            .post(&url)
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .json(&body)
            .send()
            .await
            .map_err(|e| Error::Config(format!("request failed: {e}")))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(Error::Config(format!("provider {status}: {text}")));
        }

        let mut bytes = resp.bytes_stream();
        let stream = async_stream::stream! {
            let mut buf = String::new();
            while let Some(chunk) = bytes.next().await {
                let chunk = match chunk {
                    Ok(c) => c,
                    Err(e) => { yield Err(Error::Config(format!("stream error: {e}"))); return; }
                };
                buf.push_str(&String::from_utf8_lossy(&chunk));
                while let Some(pos) = buf.find('\n') {
                    let line = buf[..pos].trim_end_matches('\r').to_string();
                    buf.drain(..=pos);
                    let data = match line.strip_prefix("data:") {
                        Some(d) => d.trim(),
                        None => continue, // 忽略 event: 行与空行
                    };
                    match parse_anthropic_delta(data) {
                        Ok(Some(text)) => yield Ok(text),
                        Ok(None) => {}
                        Err(e) => { yield Err(e); return; }
                    }
                }
            }
        };
        Ok(Box::pin(stream))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::ChatMessage;

    fn req(messages: Vec<ChatMessage>, summary: Option<&str>) -> ChatRequest {
        ChatRequest { model: "claude".into(), messages, summary: summary.map(|s| s.into()) }
    }

    #[test]
    fn system_segments_lifted_to_top_level() {
        let r = req(vec![
            ChatMessage { role: Role::System, content: "SYS".into() },
            ChatMessage { role: Role::User, content: "hi".into() },
        ], None);
        let b = anthropic_body(&r);
        assert_eq!(b["system"], "SYS");
        assert_eq!(b["messages"].as_array().unwrap().len(), 1);
        assert_eq!(b["messages"][0]["role"], "user");
        assert_eq!(b["messages"][0]["content"], "hi");
    }

    #[test]
    fn summary_prepended_to_first_user_keeps_alternation() {
        let r = req(vec![
            ChatMessage { role: Role::System, content: "SYS".into() },
            ChatMessage { role: Role::User, content: "hi".into() },
            ChatMessage { role: Role::Assistant, content: "yo".into() },
        ], Some("earlier"));
        let b = anthropic_body(&r);
        let msgs = b["messages"].as_array().unwrap();
        assert_eq!(msgs.len(), 2); // 没有额外多出一条 user
        assert_eq!(msgs[0]["role"], "user");
        let first = msgs[0]["content"].as_str().unwrap();
        assert!(first.contains("<history_summary>"));
        assert!(first.contains("earlier"));
        assert!(first.trim_end().ends_with("hi"));
        assert_eq!(msgs[1]["role"], "assistant");
    }

    #[test]
    fn parse_delta_extracts_text_block() {
        let line = r#"{"type":"content_block_delta","delta":{"type":"text_delta","text":"He"}}"#;
        assert_eq!(parse_anthropic_delta(line).unwrap(), Some("He".to_string()));
    }

    #[test]
    fn parse_delta_ignores_non_text_events() {
        assert_eq!(parse_anthropic_delta(r#"{"type":"message_start"}"#).unwrap(), None);
    }

    #[test]
    fn parse_delta_invalid_json_errors() {
        assert!(parse_anthropic_delta("not json").is_err());
    }
}
