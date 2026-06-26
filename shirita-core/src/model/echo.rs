//! Offline Echo Adapter: Streams the last user message word by word in the format `echo: <text>`.
//! Used for demonstrations and deterministic testing when no API key is available.

use async_trait::async_trait;
use futures::stream::{self, BoxStream};

use crate::models::message::Role;
use crate::Result;

use super::{ChatRequest, ModelProvider};

pub struct EchoProvider;

#[async_trait]
impl ModelProvider for EchoProvider {
    async fn stream_chat(&self, req: ChatRequest) -> Result<BoxStream<'static, Result<String>>> {
        let last_user = req
            .messages
            .iter()
            .rev()
            .find(|m| m.role == Role::User)
            .map(|m| m.content.clone())
            .unwrap_or_default();
        let reply = format!("echo: {last_user}");
        // split_inclusive preserves spaces; reassembled == reply.
        let chunks: Vec<Result<String>> = reply
            .split_inclusive(' ')
            .map(|s| Ok(s.to_string()))
            .collect();
        Ok(Box::pin(stream::iter(chunks)))
    }
}
