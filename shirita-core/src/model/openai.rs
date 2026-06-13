//! OpenAI 兼容流式适配器（POST /chat/completions, stream=true）。

use async_trait::async_trait;
use futures::stream::BoxStream;
use futures::StreamExt;

use crate::{Error, Result};

use super::{parse_delta, ChatRequest, ModelProvider};

pub struct OpenAiProvider {
    client: reqwest::Client,
    base_url: String,
    api_key: String,
}

impl OpenAiProvider {
    pub fn new(base_url: impl Into<String>, api_key: impl Into<String>) -> Self {
        Self {
            client: reqwest::Client::new(),
            base_url: base_url.into(),
            api_key: api_key.into(),
        }
    }
}

#[async_trait]
impl ModelProvider for OpenAiProvider {
    async fn stream_chat(&self, req: ChatRequest) -> Result<BoxStream<'static, Result<String>>> {
        let url = format!("{}/chat/completions", self.base_url.trim_end_matches('/'));
        let body = serde_json::json!({
            "model": req.model,
            "stream": true,
            "messages": req.messages.iter().map(|m| serde_json::json!({
                "role": m.role.as_str(),
                "content": m.content,
            })).collect::<Vec<_>>(),
        });

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
