//! OpenAI 兼容流式适配器（POST /chat/completions, stream=true）。

use async_trait::async_trait;
use futures::stream::BoxStream;
use futures::StreamExt;
use serde_json::json;

use crate::{Error, Result};

use super::{parse_delta, ChatRequest, ModelProvider};

pub struct OpenAiProvider {
    client: reqwest::Client,
    base_url: String,
    api_key: String,
}

/// 构造 OpenAI 请求体：messages 同 `openai_messages`；仅当 `req.max_tokens` 为 `Some` 时下发
/// `max_tokens`（否则用服务端默认，保持历史行为）。
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

/// 构造 OpenAI `messages` 数组：把 `req.summary`（若有）拼到首条 system 尾部；无 system 则前插一条 system。
pub fn openai_messages(req: &ChatRequest) -> Vec<serde_json::Value> {
    let mut msgs: Vec<serde_json::Value> = req
        .messages
        .iter()
        .map(|m| json!({ "role": m.role.as_str(), "content": m.content }))
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
    /// 复用共享的 `reqwest::Client`（克隆即共享连接池），避免 per-call `Client::new()`。
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

        // 把字节流解析为 content 增量流。
        let mut bytes = resp.bytes_stream();
        let stream = async_stream::stream! {
            let mut buf = String::new();
            while let Some(chunk) = bytes.next().await {
                let chunk = match chunk {
                    Ok(c) => c,
                    Err(e) => { yield Err(Error::Config(format!("stream error: {e}"))); return; }
                };
                buf.push_str(&String::from_utf8_lossy(&chunk));

                // 逐行处理已完整接收的行（以 '\n' 结尾）。
                while let Some(pos) = buf.find('\n') {
                    let line = buf[..pos].trim_end_matches('\r').to_string();
                    buf.drain(..=pos);

                    let data = match line.strip_prefix("data:") {
                        Some(d) => d.trim(),
                        None => continue, // 跳过空行/注释行
                    };
                    if data == "[DONE]" {
                        return;
                    }
                    match parse_delta(data) {
                        Ok(Some(content)) => yield Ok(content),
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
    use crate::models::message::Role;

    fn req(messages: Vec<ChatMessage>, summary: Option<&str>) -> ChatRequest {
        ChatRequest { model: "m".into(), messages, summary: summary.map(|s| s.into()), max_tokens: None }
    }

    #[test]
    fn body_includes_max_tokens_only_when_set() {
        let mut r = req(vec![ChatMessage { role: Role::User, content: "hi".into() }], None);
        assert!(openai_body(&r).get("max_tokens").is_none()); // None → 省略
        r.max_tokens = Some(8192);
        assert_eq!(openai_body(&r)["max_tokens"], 8192);
    }

    #[test]
    fn summary_appended_to_existing_system() {
        let r = req(vec![
            ChatMessage { role: Role::System, content: "SYS".into() },
            ChatMessage { role: Role::User, content: "hi".into() },
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
        let r = req(vec![ChatMessage { role: Role::User, content: "hi".into() }], Some("S"));
        let msgs = openai_messages(&r);
        assert_eq!(msgs[0]["role"], "system");
        assert!(msgs[0]["content"].as_str().unwrap().contains("S"));
    }

    #[test]
    fn no_summary_passes_through() {
        let r = req(vec![ChatMessage { role: Role::User, content: "hi".into() }], None);
        let msgs = openai_messages(&r);
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0]["content"], "hi");
    }
}
