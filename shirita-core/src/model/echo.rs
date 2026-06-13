//! 离线 Echo 适配器：把最后一条 user 消息以 `echo: <text>` 形式逐词流式回放。
//! 用于无 API key 时的演示与确定性测试。

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
        // split_inclusive 保留空格，拼回去 == reply。
        let chunks: Vec<Result<String>> = reply
            .split_inclusive(' ')
            .map(|s| Ok(s.to_string()))
            .collect();
        Ok(Box::pin(stream::iter(chunks)))
    }
}
