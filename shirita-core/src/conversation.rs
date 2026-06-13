//! 对话服务：发送消息并流式返回助手回复，结束时落库。

use std::sync::Arc;

use futures::{Stream, StreamExt};

use crate::model::{ChatMessage, ChatRequest, ModelProvider};
use crate::models::message::{Message, Role};
use crate::storage::Storage;
use crate::tokenizer::TokenCounter;

/// 流式发送过程对外暴露的事件。
#[derive(Debug, Clone, PartialEq)]
pub enum SendEvent {
    /// 一段文本增量。
    Delta(String),
    /// 完成，附助手消息 id。
    Done { message_id: String },
    /// 出错（流随后结束）。
    Error(String),
}

/// 发送一条 user 消息：落库 user → 组装历史 → 调用 provider 流式 → 累积 → 落库 assistant。
/// 返回一个事件流；assistant 消息在收到完整回复后写入存储，然后才 yield `Done`。
pub fn send_message(
    storage: Arc<dyn Storage>,
    provider: Arc<dyn ModelProvider>,
    counter: Arc<dyn TokenCounter>,
    model: String,
    session_id: String,
    user_text: String,
) -> impl Stream<Item = SendEvent> {
    async_stream::stream! {
        // 1) 落库 user 消息（parent = 当前末条消息）。
        let history = match storage.list_messages(&session_id).await {
            Ok(h) => h,
            Err(e) => { yield SendEvent::Error(e.to_string()); return; }
        };
        let last_id = history.last().map(|m| m.id.clone());
        let user_msg = Message::new(&session_id, last_id, Role::User, &user_text);
        if let Err(e) = storage.create_message(&user_msg).await {
            yield SendEvent::Error(e.to_string());
            return;
        }

        // 2) 组装请求消息（含刚落库的 user，过滤隐藏）。
        let mut chat_messages: Vec<ChatMessage> = history
            .iter()
            .filter(|m| !m.is_hidden)
            .map(|m| ChatMessage { role: m.role, content: m.raw_content.clone() })
            .collect();
        chat_messages.push(ChatMessage { role: Role::User, content: user_text.clone() });

        let prompt_text: String =
            chat_messages.iter().map(|m| m.content.as_str()).collect::<Vec<_>>().join("\n");
        tracing::debug!(prompt_tokens = counter.count(&prompt_text), "assembled prompt");

        let req = ChatRequest { model, messages: chat_messages };

        // 3) 调 provider 流，逐 delta 累积 + yield。
        let mut full = String::new();
        let mut stream = match provider.stream_chat(req).await {
            Ok(s) => s,
            Err(e) => { yield SendEvent::Error(e.to_string()); return; }
        };
        while let Some(item) = stream.next().await {
            match item {
                Ok(delta) => { full.push_str(&delta); yield SendEvent::Delta(delta); }
                Err(e) => { yield SendEvent::Error(e.to_string()); return; }
            }
        }

        // 4) 落库 assistant 消息，再 yield Done。
        let assistant = Message::new(&session_id, Some(user_msg.id.clone()), Role::Assistant, &full);
        if let Err(e) = storage.create_message(&assistant).await {
            yield SendEvent::Error(e.to_string());
            return;
        }
        yield SendEvent::Done { message_id: assistant.id };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::EchoProvider;
    use crate::models::session::Session;
    use crate::storage::sqlite::SqliteStorage;
    use crate::tokenizer::tiktoken::TiktokenCounter;

    async fn temp_storage() -> SqliteStorage {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("conv.db");
        std::mem::forget(dir);
        let s = SqliteStorage::connect(path.to_str().unwrap()).await.unwrap();
        s.run_migrations().await.unwrap();
        s
    }

    #[tokio::test]
    async fn echo_send_streams_and_persists() {
        let storage = Arc::new(temp_storage().await);
        let session = Session::new("t");
        storage.create_session(&session).await.unwrap();

        let storage_dyn: Arc<dyn Storage> = storage.clone();
        let provider: Arc<dyn ModelProvider> = Arc::new(EchoProvider);
        let counter: Arc<dyn TokenCounter> = Arc::new(TiktokenCounter::new());

        let stream = send_message(
            storage_dyn,
            provider,
            counter,
            "test-model".into(),
            session.id.clone(),
            "hello".into(),
        );
        futures::pin_mut!(stream);

        let mut deltas = String::new();
        let mut done_id = None;
        while let Some(ev) = stream.next().await {
            match ev {
                SendEvent::Delta(d) => deltas.push_str(&d),
                SendEvent::Done { message_id } => done_id = Some(message_id),
                SendEvent::Error(e) => panic!("unexpected error: {e}"),
            }
        }
        assert_eq!(deltas, "echo: hello");
        assert!(done_id.is_some());

        // 持久化校验：user + assistant 各一条，内容正确。
        let msgs = storage.list_messages(&session.id).await.unwrap();
        assert_eq!(msgs.len(), 2);
        assert_eq!(msgs[0].role, Role::User);
        assert_eq!(msgs[0].raw_content, "hello");
        assert_eq!(msgs[1].role, Role::Assistant);
        assert_eq!(msgs[1].raw_content, "echo: hello");
        assert_eq!(msgs[1].parent_id.as_deref(), Some(msgs[0].id.as_str()));
    }
}
