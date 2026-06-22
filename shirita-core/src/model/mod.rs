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
    /// Attached images as data URLs (`data:image/png;base64,...`), resolved
    /// from stored asset ids ahead of request assembly (see
    /// `attachments::resolve_images`). Empty for plain-text turns.
    pub images: Vec<String>,
}

impl Default for ChatMessage {
    fn default() -> Self {
        Self { role: Role::User, content: String::new(), images: Vec::new() }
    }
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

/// 一个解析出的 OpenAI 兼容 delta：可见内容，或推理模型（如 DeepSeek）原生的
/// `reasoning_content` 思考增量。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Delta {
    Content(String),
    Reasoning(String),
    None,
}

/// 解析 OpenAI 兼容 SSE 的 `delta`：优先识别 `reasoning_content`（DeepSeek 等原生推理字段），
/// 否则取 `content`；都没有（如仅 role 的首帧）则 `Delta::None`。
pub fn parse_delta_kind(json_after_data: &str) -> Result<Delta> {
    let v: serde_json::Value = serde_json::from_str(json_after_data)?;
    let delta = &v["choices"][0]["delta"];
    if let Some(r) = delta["reasoning_content"].as_str() {
        return Ok(Delta::Reasoning(r.to_string()));
    }
    if let Some(c) = delta["content"].as_str() {
        return Ok(Delta::Content(c.to_string()));
    }
    Ok(Delta::None)
}

/// 把网络分片字节流安全地解码为 UTF-8：跨分片被截断的多字节字符不会被
/// 当场替换成 U+FFFD，而是留在 `pending` 里等下一片字节补全后再解码。
/// 真正非法的字节序列才会被替换并跳过（不会无限堆积在 `pending` 里）。
pub fn decode_utf8_chunk(pending: &mut Vec<u8>, chunk: &[u8]) -> String {
    pending.extend_from_slice(chunk);
    let mut out = String::new();
    loop {
        match std::str::from_utf8(pending) {
            Ok(s) => {
                out.push_str(s);
                pending.clear();
                break;
            }
            Err(e) => {
                let valid_up_to = e.valid_up_to();
                out.push_str(std::str::from_utf8(&pending[..valid_up_to]).expect("valid_up_to guarantees this prefix is valid UTF-8"));
                match e.error_len() {
                    Some(bad_len) => {
                        // A genuinely invalid byte sequence (not just a
                        // chunk boundary mid-character) — replace and skip
                        // past it, then keep decoding the remainder.
                        out.push('\u{FFFD}');
                        pending.drain(..valid_up_to + bad_len);
                    }
                    None => {
                        // Trailing bytes are an incomplete sequence; hold
                        // them for the next chunk and stop for now.
                        pending.drain(..valid_up_to);
                        break;
                    }
                }
            }
        }
    }
    out
}

/// 把一个 `Delta` 渲染成要 yield 的文本增量，沿用既有的 `<think>…</think>` 前端折叠约定
/// （见 `shirita-ui/src/utils/thinking.ts`），在推理段与正文段切换时补上开/闭标签。
/// `in_reasoning` 在调用间持有状态（每个流一个），纯函数便于单测。
pub fn render_delta(in_reasoning: &mut bool, delta: Delta) -> Option<String> {
    match delta {
        Delta::Reasoning(t) => {
            let prefix = if *in_reasoning { "" } else { "<think>" };
            *in_reasoning = true;
            Some(format!("{prefix}{t}"))
        }
        Delta::Content(t) => {
            let prefix = if *in_reasoning { "</think>" } else { "" };
            *in_reasoning = false;
            Some(format!("{prefix}{t}"))
        }
        Delta::None => None,
    }
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

    #[test]
    fn parse_delta_kind_prefers_reasoning_content() {
        let line = r#"{"choices":[{"delta":{"reasoning_content":"hm"}}]}"#;
        assert_eq!(parse_delta_kind(line).unwrap(), Delta::Reasoning("hm".to_string()));
    }

    #[test]
    fn parse_delta_kind_falls_back_to_content() {
        let line = r#"{"choices":[{"delta":{"content":"He"}}]}"#;
        assert_eq!(parse_delta_kind(line).unwrap(), Delta::Content("He".to_string()));
    }

    #[test]
    fn parse_delta_kind_role_only_is_none() {
        let line = r#"{"choices":[{"delta":{"role":"assistant"}}]}"#;
        assert_eq!(parse_delta_kind(line).unwrap(), Delta::None);
    }

    #[test]
    fn render_delta_wraps_reasoning_run_in_think_tags() {
        let mut in_reasoning = false;
        assert_eq!(render_delta(&mut in_reasoning, Delta::Reasoning("a".into())), Some("<think>a".to_string()));
        assert!(in_reasoning);
        assert_eq!(render_delta(&mut in_reasoning, Delta::Reasoning("b".into())), Some("b".to_string()));
        assert_eq!(render_delta(&mut in_reasoning, Delta::Content("c".into())), Some("</think>c".to_string()));
        assert!(!in_reasoning);
        assert_eq!(render_delta(&mut in_reasoning, Delta::Content("d".into())), Some("d".to_string()));
    }

    #[test]
    fn render_delta_plain_content_has_no_tags() {
        let mut in_reasoning = false;
        assert_eq!(render_delta(&mut in_reasoning, Delta::Content("hi".into())), Some("hi".to_string()));
        assert!(!in_reasoning);
    }

    #[test]
    fn render_delta_none_yields_nothing() {
        let mut in_reasoning = false;
        assert_eq!(render_delta(&mut in_reasoning, Delta::None), None);
    }

    #[test]
    fn decode_utf8_chunk_handles_whole_chunks() {
        let mut pending = Vec::new();
        assert_eq!(decode_utf8_chunk(&mut pending, "hello".as_bytes()), "hello");
        assert!(pending.is_empty());
    }

    #[test]
    fn decode_utf8_chunk_reassembles_a_multibyte_char_split_across_chunks() {
        // "café" — 'é' is the 2-byte sequence 0xC3 0xA9. Split it down the middle.
        let bytes = "café".as_bytes();
        let (first, second) = bytes.split_at(bytes.len() - 1);
        let mut pending = Vec::new();
        let out1 = decode_utf8_chunk(&mut pending, first);
        assert_eq!(out1, "caf", "incomplete trailing byte must not be lossily replaced yet");
        assert_eq!(pending, vec![0xC3], "the lone lead byte stays buffered");
        let out2 = decode_utf8_chunk(&mut pending, second);
        assert_eq!(out2, "é");
        assert!(pending.is_empty());
    }

    #[test]
    fn decode_utf8_chunk_replaces_genuinely_invalid_bytes() {
        let mut pending = Vec::new();
        // 0xFF is never valid in UTF-8 on its own.
        let out = decode_utf8_chunk(&mut pending, &[b'a', 0xFF, b'b']);
        assert_eq!(out, "a\u{FFFD}b");
        assert!(pending.is_empty());
    }
}
