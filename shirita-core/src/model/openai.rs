//! OpenAI-compatible streaming adapter (POST /chat/completions, stream=true).

use async_trait::async_trait;
use futures::stream::BoxStream;
use futures::StreamExt;
use serde_json::json;

use crate::{Error, Result};

use super::{close_reasoning, decode_utf8_chunk, parse_delta_kind, render_delta, ChatRequest, ModelProvider};

pub struct OpenAiProvider {
    client: reqwest::Client,
    base_url: String,
    api_key: String,
}

/// Construct the OpenAI request body: `messages` is the same as `openai_messages`; send only when `req.max_tokens` is `Some`
/// `max_tokens` (otherwise, use the server default to maintain historical behavior).
pub fn openai_body(req: &ChatRequest) -> serde_json::Value {
    let mut body = json!({
        "model": req.model,
        "stream": true,
        "messages": openai_messages(req),
    });
    if let Some(mt) = req.max_tokens {
        body["max_tokens"] = json!(mt);
    }
    body
}

/// OpenAI `content` for a single message: a plain string if there is no image; `[{type:text}, {type:image_url}...]` if there is an image.
/// `content-parts` array (vision format). The `text` part is omitted when empty text is paired with an image.
fn openai_content(m: &super::ChatMessage) -> serde_json::Value {
    if m.images.is_empty() {
        return json!(m.content);
    }
    let mut parts: Vec<serde_json::Value> = Vec::new();
    if !m.content.is_empty() {
        parts.push(json!({ "type": "text", "text": m.content }));
    }
    for url in &m.images {
        parts.push(json!({ "type": "image_url", "image_url": { "url": url } }));
    }
    json!(parts)
}

/// Construct the OpenAI `messages` array: append `req.summary` (if present) to the end of the first `system` message; if there is no `system` message, insert a `system` message at the beginning.
pub fn openai_messages(req: &ChatRequest) -> Vec<serde_json::Value> {
    let mut msgs: Vec<serde_json::Value> = req
        .messages
        .iter()
        .map(|m| json!({ "role": m.role.as_str(), "content": openai_content(m) }))
        .collect();
    if let Some(sum) = &req.summary {
        let block = format!("\n\n[Summary of earlier conversation]\n{sum}");
        if let Some(sys) = msgs.iter_mut().find(|m| m["role"] == "system") {
            let cur = sys["content"].as_str().unwrap_or("").to_string();
            sys["content"] = json!(format!("{cur}{block}"));
        } else {
            msgs.insert(0, json!({ "role": "system", "content": block.trim_start() }));
        }
    }
    msgs
}

impl OpenAiProvider {
    /// Reuse a shared `request::Client` (cloning it shares the connection pool) to avoid calling `Client::new()` for each request.
    pub fn new(
        client: reqwest::Client,
        base_url: impl Into<String>,
        api_key: impl Into<String>,
    ) -> Self {
        Self {
            client,
            base_url: base_url.into(),
            api_key: api_key.into(),
        }
    }
}

#[async_trait]
impl ModelProvider for OpenAiProvider {
    async fn stream_chat(&self, req: ChatRequest) -> Result<BoxStream<'static, Result<String>>> {
        let url = format!("{}/chat/completions", self.base_url.trim_end_matches('/'));
        let body = openai_body(&req);

        let resp = self
            .client
            .post(&url)
            .bearer_auth(&self.api_key)
            .json(&body)
            .send()
            .await
            .map_err(|e| Error::Config(format!("request failed: {e}")))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(Error::Config(format!("provider {status}: {text}")));
        }

        // Parse the byte stream into an incremental content stream.
        let mut bytes = resp.bytes_stream();
        let stream = async_stream::stream! {
            let mut buf = String::new();
            let mut pending_bytes = Vec::new();
            // Inference models such as DeepSeek stream `reasoning_content` before `content`;
            // This section wraps it into the existing <think>…</think> frontend convention (see model/mod.rs::render_delta).
            let mut in_reasoning = false;
            while let Some(chunk) = bytes.next().await {
                let chunk = match chunk {
                    Ok(c) => c,
                    Err(e) => { yield Err(Error::Config(format!("stream error: {e}"))); return; }
                };
                // Multi-byte characters may be split exactly between two chunk boundaries; use `decode_utf8_chunk`
                // instead of applying `from_utf8_lossy` to each chunk individually, to avoid converting truncated characters into garbled text.
                buf.push_str(&decode_utf8_chunk(&mut pending_bytes, &chunk));

                 // Process fully received lines (terminated by ‘\n’) one by one.
                while let Some(pos) = buf.find('\n') {
                    let line = buf[..pos].trim_end_matches('\r').to_string();
                    buf.drain(..=pos);

                    let data = match line.strip_prefix("data:") {
                        Some(d) => d.trim(),
                        None => continue,
                    };
                    if data == "[DONE]" {
                        if let Some(close) = close_reasoning(&mut in_reasoning) { yield Ok(close); }
                        return;
                    }
                    match parse_delta_kind(data) {
                        Ok(delta) => if let Some(text) = render_delta(&mut in_reasoning, delta) { yield Ok(text); },
                        Err(e) => { yield Err(e); return; }
                    }
                }
            }
            // Clean EOF without a trailing [DONE]: still close a dangling <think>.
            if let Some(close) = close_reasoning(&mut in_reasoning) { yield Ok(close); }
        };
        Ok(Box::pin(stream))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::ChatMessage;
    use crate::models::message::Role;

    fn req(messages: Vec<ChatMessage>, summary: Option<&str>) -> ChatRequest {
        ChatRequest { model: "m".into(), messages, summary: summary.map(|s| s.into()), max_tokens: None }
    }

    #[test]
    fn body_includes_max_tokens_only_when_set() {
        let mut r = req(vec![ChatMessage { role: Role::User, content: "hi".into(), ..Default::default() }], None);
        assert!(openai_body(&r).get("max_tokens").is_none()); // None → ignore
        r.max_tokens = Some(8192);
        assert_eq!(openai_body(&r)["max_tokens"], 8192);
    }

    #[test]
    fn summary_appended_to_existing_system() {
        let r = req(vec![
            ChatMessage { role: Role::System, content: "SYS".into(), ..Default::default() },
            ChatMessage { role: Role::User, content: "hi".into(), ..Default::default() },
        ], Some("earlier stuff"));
        let msgs = openai_messages(&r);
        assert_eq!(msgs[0]["role"], "system");
        let sys = msgs[0]["content"].as_str().unwrap();
        assert!(sys.starts_with("SYS"));
        assert!(sys.contains("earlier stuff"));
        assert_eq!(msgs[1]["content"], "hi");
    }

    #[test]
    fn summary_prepends_system_when_none() {
        let r = req(vec![ChatMessage { role: Role::User, content: "hi".into(), ..Default::default() }], Some("S"));
        let msgs = openai_messages(&r);
        assert_eq!(msgs[0]["role"], "system");
        assert!(msgs[0]["content"].as_str().unwrap().contains("S"));
    }

    #[test]
    fn no_summary_passes_through() {
        let r = req(vec![ChatMessage { role: Role::User, content: "hi".into(), ..Default::default() }], None);
        let msgs = openai_messages(&r);
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0]["content"], "hi");
    }

    #[test]
    fn message_with_image_uses_content_parts_array() {
        let r = req(vec![ChatMessage {
            role: Role::User,
            content: "what is this?".into(),
            images: vec!["data:image/png;base64,AAA".into()],
        }], None);
        let msgs = openai_messages(&r);
        let parts = msgs[0]["content"].as_array().unwrap();
        assert_eq!(parts[0], json!({ "type": "text", "text": "what is this?" }));
        assert_eq!(parts[1], json!({ "type": "image_url", "image_url": { "url": "data:image/png;base64,AAA" } }));
    }

    #[test]
    fn image_only_message_omits_empty_text_part() {
        let r = req(vec![ChatMessage {
            role: Role::User,
            content: "".into(),
            images: vec!["data:image/png;base64,AAA".into()],
        }], None);
        let msgs = openai_messages(&r);
        let parts = msgs[0]["content"].as_array().unwrap();
        assert_eq!(parts.len(), 1);
        assert_eq!(parts[0]["type"], "image_url");
    }
}
