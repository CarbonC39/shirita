//! Model adaptation layer: Unified streaming chat interface.

pub mod anthropic;
pub mod echo;
pub mod openai;

use async_trait::async_trait;
use futures::stream::BoxStream;

use crate::models::message::Role;
use crate::{Error, Result};

pub use anthropic::AnthropicProvider;
pub use echo::EchoProvider;
pub use openai::OpenAiProvider;

/// A single message sent to the model
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

/// A single chat autocomplete request.
#[derive(Debug, Clone)]
pub struct ChatRequest {
    pub model: String,
    pub messages: Vec<ChatMessage>,
    /// Rolling summary for the current branch (if available). The location within the request body is determined by each provider.
    pub summary: Option<String>,
    /// Maximum number of tokens in the response (output). Non-contextual window. When `None`, Anthropic uses the built-in default,
    /// OpenAI omits this field (uses the server-side default). Source: `provider_max_tokens` setting.
    pub max_tokens: Option<u32>,
}

/// Stream-based chat: Each element is a text increment; when the stream ends, the process is complete.
#[async_trait]
pub trait ModelProvider: Send + Sync {
    async fn stream_chat(&self, req: ChatRequest) -> Result<BoxStream<'static, Result<String>>>;
}

/// Parses the JSON following `data:` in OpenAI SSE and extracts `choices[0].delta.content`.
/// Returns `Ok(None)` for the first frame that contains only a role (no content).
pub fn parse_delta(json_after_data: &str) -> Result<Option<String>> {
    let v: serde_json::Value = serde_json::from_str(json_after_data)?;
    Ok(v["choices"][0]["delta"]["content"]
        .as_str()
        .map(|s| s.to_string()))
}

/// A parsed OpenAI-compatible delta: visible content, or the `reasoning_content` reasoning increment native to inference models (such as DeepSeek).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Delta {
    Content(String),
    Reasoning(String),
    None,
}

/// Parse OpenAI's SSE-compatible `delta`: First check for `reasoning_content` (native reasoning fields such as DeepSeek),
/// otherwise use `content`; if neither is present (e.g., the first frame of a role-only response), return `Delta::None`.
pub fn parse_delta_kind(json_after_data: &str) -> Result<Delta> {
    let v: serde_json::Value = serde_json::from_str(json_after_data)?;
    // Some OpenAI-compatible providers report failures as an in-stream
    // `{"error": {...}}` frame (HTTP 200). Surface it instead of swallowing it
    // as Delta::None — but ignore a benign `"error": null` carried on a normal frame.
    if let Some(err) = v.get("error").filter(|e| !e.is_null()) {
        let msg = err.get("message").and_then(|m| m.as_str()).unwrap_or("provider error");
        return Err(Error::Config(format!("provider stream error: {msg}")));
    }
    let delta = &v["choices"][0]["delta"];
    if let Some(r) = delta["reasoning_content"].as_str() {
        return Ok(Delta::Reasoning(r.to_string()));
    }
    if let Some(c) = delta["content"].as_str() {
        return Ok(Delta::Content(c.to_string()));
    }
    Ok(Delta::None)
}

/// Safely decode a network-segmented byte stream into UTF-8: multi-byte characters that are truncated across segments are not
/// immediately replaced with U+FFFD, but are left in `pending` to be decoded once the next segment of bytes is received.
/// Only truly invalid byte sequences are replaced and skipped (so they do not accumulate indefinitely in `pending`).
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

/// Render a `Delta` as the text increment to be yielded, following the existing `<think>…</think>` front-end folding convention
/// (see `shirita-ui/src/utils/thinking.ts`), adding opening and closing tags when switching between reasoning and body paragraphs.
/// `in_reasoning` maintains state between calls (one per stream); being a pure function makes it easy to unit test.
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

/// Close an open `<think>` run at end of stream. When a model streams
/// `reasoning_content` and the stream ends (clean EOF or `[DONE]`) before any
/// `content` delta arrives, `render_delta` never emits the closing tag — leaving
/// a dangling `<think>`. Adapters call this on every clean exit. Idempotent.
pub fn close_reasoning(in_reasoning: &mut bool) -> Option<String> {
    if *in_reasoning {
        *in_reasoning = false;
        Some("</think>".to_string())
    } else {
        None
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
    fn parse_delta_kind_surfaces_mid_stream_error_frame() {
        // Some OpenAI-compatible providers send `data: {"error": {...}}` mid-stream;
        // it must be surfaced, not silently swallowed as Delta::None.
        let line = r#"{"error":{"message":"rate limit exceeded"}}"#;
        let err = parse_delta_kind(line).unwrap_err().to_string();
        assert!(err.contains("rate limit exceeded"), "got: {err}");
    }

    #[test]
    fn parse_delta_kind_ignores_null_error_field() {
        // A normal frame that carries `"error": null` is not an error.
        let line = r#"{"choices":[{"delta":{"content":"hi"}}],"error":null}"#;
        assert_eq!(parse_delta_kind(line).unwrap(), Delta::Content("hi".to_string()));
    }

    #[test]
    fn close_reasoning_closes_open_run_once() {
        let mut in_reasoning = true;
        assert_eq!(close_reasoning(&mut in_reasoning), Some("</think>".to_string()));
        assert!(!in_reasoning);
        assert_eq!(close_reasoning(&mut in_reasoning), None); // idempotent
    }

    #[test]
    fn close_reasoning_noop_when_not_reasoning() {
        let mut in_reasoning = false;
        assert_eq!(close_reasoning(&mut in_reasoning), None);
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
