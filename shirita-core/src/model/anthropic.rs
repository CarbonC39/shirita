//! Anthropic Messages API streaming adapter (POST /v1/messages, stream=true).

use async_trait::async_trait;
use futures::stream::BoxStream;
use futures::StreamExt;
use serde_json::json;

use crate::models::message::Role;
use crate::{Error, Result};

use super::{decode_utf8_chunk, ChatMessage, ChatRequest, ModelProvider};

pub struct AnthropicProvider {
    client: reqwest::Client,
    base_url: String,
    api_key: String,
}

impl AnthropicProvider {
    /// Reuse a shared `request::Client` (cloning it shares the connection pool) to avoid calling `Client::new()` for each request.
    pub fn new(
        client: reqwest::Client,
        base_url: impl Into<String>,
        api_key: impl Into<String>,
    ) -> Self {
        Self { client, base_url: base_url.into(), api_key: api_key.into() }
    }
}

/// The Anthropic `content` for a single message: a plain string if there are no images; if there are images, it is
/// an array of content blocks in the format `[{type:text}, {type:image, source:{type:base64,...}}...]`.
/// Images are passed as data URLs (`data:<mime>;base64,<data>`), and the media_type and data are extracted using `;base64,` as a delimiter.
fn anthropic_content(m: &ChatMessage) -> serde_json::Value {
    if m.images.is_empty() {
        return json!(m.content);
    }
    let mut parts: Vec<serde_json::Value> = Vec::new();
    if !m.content.is_empty() {
        parts.push(json!({ "type": "text", "text": m.content }));
    }
    for url in &m.images {
        if let Some((media_type, data)) = split_data_url(url) {
            parts.push(json!({
                "type": "image",
                "source": { "type": "base64", "media_type": media_type, "data": data },
            }));
        }
    }
    json!(parts)
}

fn split_data_url(url: &str) -> Option<(&str, &str)> {
    url.strip_prefix("data:")?.split_once(";base64,")
}

/// Insert the summary text before the content of a message: concatenate the string directly to the content;
/// For an array of content parts (with images), insert a text part at the beginning of the array, while preserving the image blocks.
fn prepend_text(content: &mut serde_json::Value, prefix: &str) {
    if let Some(s) = content.as_str() {
        *content = json!(format!("{prefix}\n\n{s}"));
    } else if let Some(arr) = content.as_array_mut() {
        arr.insert(0, json!({ "type": "text", "text": prefix }));
    }
}

/// Construct the Anthropic request body: Merge the System segment into the top-level `system`; wrap `summary` in `<history_summary>`
/// **Prepend to the beginning of the first user message** (maintain an alternating pattern of user/assistant to avoid consecutive messages from the same role); if there is no user message, treat this as the first user message.
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
            Role::User => messages.push(json!({ "role": "user", "content": anthropic_content(m) })),
            Role::Assistant => messages.push(json!({ "role": "assistant", "content": anthropic_content(m) })),
        }
    }
    if let Some(sum) = &req.summary {
        let wrapped = format!("<history_summary>\n{sum}\n</history_summary>");
        if let Some(first_user) = messages.iter_mut().find(|m| m["role"] == "user") {
            prepend_text(&mut first_user["content"], &wrapped);
        } else {
            messages.insert(0, json!({ "role": "user", "content": wrapped }));
        }
    }
    json!({
        "model": req.model,
        "stream": true,
        "max_tokens": req.max_tokens.unwrap_or(8192),
        "system": system,
        "messages": messages,
    })
}

/// Parsed Anthropic SSE events: the start and body of a thinking block, or the body of a text block; all other events are ignored.
/// “extended thinking” uses a separate `thinking` content block type (`content_block_start` →
/// multiple `thinking_delta` → implicitly ends at the start of the next block), while text blocks use `text_delta`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AnthropicEvent {
    ThinkingStart,
    Thinking(String),
    Text(String),
    Other,
}

/// Parse the JSON following `data:` in Anthropic SSE into an array of [`AnthropicEvent`] objects.
pub fn parse_anthropic_event(json_after_data: &str) -> Result<AnthropicEvent> {
    let v: serde_json::Value = serde_json::from_str(json_after_data)?;
    match v["type"].as_str().unwrap_or("") {
        "content_block_start" => match v["content_block"]["type"].as_str().unwrap_or("") {
            "thinking" => Ok(AnthropicEvent::ThinkingStart),
            _ => Ok(AnthropicEvent::Other),
        },
        "content_block_delta" => match v["delta"]["type"].as_str().unwrap_or("") {
            "thinking_delta" => Ok(AnthropicEvent::Thinking(
                v["delta"]["thinking"].as_str().unwrap_or("").to_string(),
            )),
            "text_delta" => Ok(AnthropicEvent::Text(
                v["delta"]["text"].as_str().unwrap_or("").to_string(),
            )),
            _ => Ok(AnthropicEvent::Other),
        },
        _ => Ok(AnthropicEvent::Other),
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
            let mut pending_bytes = Vec::new();
            // The “extended thinking” block is streamed using separate content_block_start and _delta events;
            // It is nested within the existing <think>…</think> front-end convention (see thinking.ts), and a closing tag is added when the next text block begins.
            let mut in_thinking = false;
            while let Some(chunk) = bytes.next().await {
                let chunk = match chunk {
                    Ok(c) => c,
                    Err(e) => { yield Err(Error::Config(format!("stream error: {e}"))); return; }
                };
                // Multi-byte characters may fall exactly between two chunk boundaries; see model/mod.rs::decode_utf8_chunk.
                buf.push_str(&decode_utf8_chunk(&mut pending_bytes, &chunk));
                while let Some(pos) = buf.find('\n') {
                    let line = buf[..pos].trim_end_matches('\r').to_string();
                    buf.drain(..=pos);
                    let data = match line.strip_prefix("data:") {
                        Some(d) => d.trim(),
                        None => continue,
                    };
                    match parse_anthropic_event(data) {
                        Ok(AnthropicEvent::ThinkingStart) => {
                            in_thinking = true;
                            yield Ok("<think>".to_string());
                        }
                        Ok(AnthropicEvent::Thinking(t)) => yield Ok(t),
                        Ok(AnthropicEvent::Text(t)) => {
                            if in_thinking {
                                in_thinking = false;
                                yield Ok(format!("</think>{t}"));
                            } else {
                                yield Ok(t);
                            }
                        }
                        Ok(AnthropicEvent::Other) => {}
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
        ChatRequest { model: "claude".into(), messages, summary: summary.map(|s| s.into()), max_tokens: None }
    }

    #[test]
    fn body_max_tokens_defaults_to_8192_and_honors_override() {
        let mut r = req(vec![ChatMessage { role: Role::User, content: "hi".into(), ..Default::default() }], None);
        assert_eq!(anthropic_body(&r)["max_tokens"], 8192); // None → default 8192
        r.max_tokens = Some(2000);
        assert_eq!(anthropic_body(&r)["max_tokens"], 2000);
    }

    #[test]
    fn system_segments_lifted_to_top_level() {
        let r = req(vec![
            ChatMessage { role: Role::System, content: "SYS".into(), ..Default::default() },
            ChatMessage { role: Role::User, content: "hi".into(), ..Default::default() },
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
            ChatMessage { role: Role::System, content: "SYS".into(), ..Default::default() },
            ChatMessage { role: Role::User, content: "hi".into(), ..Default::default() },
            ChatMessage { role: Role::Assistant, content: "yo".into(), ..Default::default() },
        ], Some("earlier"));
        let b = anthropic_body(&r);
        let msgs = b["messages"].as_array().unwrap();
        assert_eq!(msgs.len(), 2);
        assert_eq!(msgs[0]["role"], "user");
        let first = msgs[0]["content"].as_str().unwrap();
        assert!(first.contains("<history_summary>"));
        assert!(first.contains("earlier"));
        assert!(first.trim_end().ends_with("hi"));
        assert_eq!(msgs[1]["role"], "assistant");
    }

    #[test]
    fn parse_event_extracts_text_block() {
        let line = r#"{"type":"content_block_delta","delta":{"type":"text_delta","text":"He"}}"#;
        assert_eq!(parse_anthropic_event(line).unwrap(), AnthropicEvent::Text("He".to_string()));
    }

    #[test]
    fn parse_event_ignores_non_text_events() {
        assert_eq!(parse_anthropic_event(r#"{"type":"message_start"}"#).unwrap(), AnthropicEvent::Other);
    }

    #[test]
    fn parse_event_invalid_json_errors() {
        assert!(parse_anthropic_event("not json").is_err());
    }

    #[test]
    fn parse_event_thinking_block_start() {
        let line = r#"{"type":"content_block_start","content_block":{"type":"thinking"}}"#;
        assert_eq!(parse_anthropic_event(line).unwrap(), AnthropicEvent::ThinkingStart);
    }

    #[test]
    fn parse_event_text_block_start_is_other() {
        let line = r#"{"type":"content_block_start","content_block":{"type":"text"}}"#;
        assert_eq!(parse_anthropic_event(line).unwrap(), AnthropicEvent::Other);
    }

    #[test]
    fn parse_event_extracts_thinking_delta() {
        let line = r#"{"type":"content_block_delta","delta":{"type":"thinking_delta","thinking":"hm..."}}"#;
        assert_eq!(parse_anthropic_event(line).unwrap(), AnthropicEvent::Thinking("hm...".to_string()));
    }

    #[test]
    fn message_with_image_uses_content_blocks_array() {
        let r = req(vec![ChatMessage {
            role: Role::User,
            content: "what is this?".into(),
            images: vec!["data:image/png;base64,AAA".into()],
        }], None);
        let b = anthropic_body(&r);
        let parts = b["messages"][0]["content"].as_array().unwrap();
        assert_eq!(parts[0], json!({ "type": "text", "text": "what is this?" }));
        assert_eq!(
            parts[1],
            json!({ "type": "image", "source": { "type": "base64", "media_type": "image/png", "data": "AAA" } })
        );
    }

    #[test]
    fn image_only_message_omits_empty_text_block() {
        let r = req(vec![ChatMessage {
            role: Role::User,
            content: "".into(),
            images: vec!["data:image/png;base64,AAA".into()],
        }], None);
        let b = anthropic_body(&r);
        let parts = b["messages"][0]["content"].as_array().unwrap();
        assert_eq!(parts.len(), 1);
        assert_eq!(parts[0]["type"], "image");
    }

    #[test]
    fn summary_prepends_a_text_block_when_first_user_has_images() {
        let r = req(vec![ChatMessage {
            role: Role::User,
            content: "what is this?".into(),
            images: vec!["data:image/png;base64,AAA".into()],
        }], Some("earlier"));
        let b = anthropic_body(&r);
        let parts = b["messages"][0]["content"].as_array().unwrap();
        // summary text block prepended, image block preserved
        assert_eq!(parts.len(), 3);
        assert_eq!(parts[0]["type"], "text");
        assert!(parts[0]["text"].as_str().unwrap().contains("earlier"));
        assert_eq!(parts[2]["type"], "image");
    }
}
