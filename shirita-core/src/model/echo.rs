//! Offline Echo Adapter: Streams the last user message word by word in the format `echo: <text>`.
//! Used for demonstrations and deterministic testing when no API key is available.

use async_trait::async_trait;
use futures::stream::{self, BoxStream};

use crate::models::message::Role;
use crate::tools::{ToolCall, ToolCallTransport};
use crate::Result;

use super::{ChatRequest, ModelEvent, ModelProvider};

pub struct EchoProvider;

#[async_trait]
impl ModelProvider for EchoProvider {
    async fn stream_chat(&self, req: ChatRequest) -> Result<BoxStream<'static, Result<ModelEvent>>> {
        if let Some(result) = req.messages.iter().rev().find_map(|m| m.tool_result.as_ref()) {
            return Ok(Box::pin(stream::iter(vec![
                Ok(ModelEvent::TextDelta(format!("echo tool result: {}", result.output))),
                Ok(ModelEvent::Finished { reason: "stop".into() }),
            ])));
        }
        let last_user = req
            .messages
            .iter()
            .rev()
            .find(|m| m.role == Role::User)
            .map(|m| m.content.clone())
            .unwrap_or_default();
        if let Some(script) = last_user.strip_prefix("echo:tool:") {
            if let Some((name, raw)) = script.split_once(':') {
                let events = vec![
                    Ok(ModelEvent::ToolCall(ToolCall { id: "echo-call-1".into(), name: name.into(), arguments: serde_json::from_str(raw).unwrap_or(serde_json::Value::Null), transport: ToolCallTransport::Native })),
                    Ok(ModelEvent::Finished { reason: "tool_calls".into() }),
                ];
                return Ok(Box::pin(stream::iter(events)));
            }
        }
        let reply = format!("echo: {last_user}");
        // split_inclusive preserves spaces; reassembled == reply.
        let mut chunks: Vec<Result<ModelEvent>> = reply
            .split_inclusive(' ')
            .map(|s| Ok(ModelEvent::TextDelta(s.to_string())))
            .collect();
        chunks.push(Ok(ModelEvent::Finished { reason: "stop".into() }));
        Ok(Box::pin(stream::iter(chunks)))
    }
}
