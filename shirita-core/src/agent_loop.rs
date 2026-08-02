//! Provider-neutral multi-round RP Agent execution.

use std::collections::HashSet;
use std::sync::Arc;

use async_stream::stream;
use futures::{Stream, StreamExt};

use crate::agent::{AgentSettings, GenerationRun, RunStatus, ToolTransport};
use crate::conversation::StopToken;
use crate::model::{ChatMessage, ChatRequest, ModelEvent, ModelProvider};
use crate::models::message::Role;
use crate::tools::{
    ToolCall, ToolCallTransport, ToolControl, ToolRegistry, ToolResult, ToolResultStatus,
};
use crate::xml_tools::{parse_xml_tool_round, render_xml_tool_result, xml_protocol_prompt};

#[derive(Debug, Clone, PartialEq)]
pub enum HarnessEvent {
    RoundStarted { run: GenerationRun },
    ToolStarted {
        call: ToolCall,
    },
    ToolFinished {
        result: ToolResult,
    },
    Status {
        message: String,
    },
    Usage {
        input_tokens: u64,
        output_tokens: u64,
    },
    Finished { response: String, run: GenerationRun },
    Stopped { run: GenerationRun },
    Failed {
        message: String,
    },
}

fn selected_specs(registry: &ToolRegistry, enabled: &[String]) -> Vec<crate::tools::ToolSpec> {
    registry
        .specs()
        .into_iter()
        .filter(|spec| spec.required || enabled.contains(&spec.name))
        .collect()
}

fn call_signature(calls: &[ToolCall]) -> String {
    calls
        .iter()
        .map(|call| {
            format!(
                "{}:{}",
                call.name,
                serde_json::to_string(&call.arguments).unwrap_or_default()
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Execute a generation without persisting anything. Private model text stays
/// inside this stream; only a successful `shirita.run.finish` emits `Finished`.
pub fn run(
    provider: Arc<dyn ModelProvider>,
    mut request: ChatRequest,
    settings: AgentSettings,
    native_supported: bool,
    registry: Arc<ToolRegistry>,
    stop: StopToken,
    mut run_state: GenerationRun,
) -> impl Stream<Item = HarnessEvent> {
    stream! {
        let transport = match settings.transport {
            ToolTransport::Xml => ToolCallTransport::Xml,
            ToolTransport::Native if native_supported => ToolCallTransport::Native,
            ToolTransport::Native => {
                yield HarnessEvent::Failed { message: "native Tool calling is disabled for this provider".into() };
                return;
            }
            ToolTransport::Auto if native_supported => ToolCallTransport::Native,
            ToolTransport::Auto => ToolCallTransport::Xml,
        };
        let specs = selected_specs(&registry, &settings.enabled_tools);
        let mut harness_prompt = settings.system_prompt.clone();
        if transport == ToolCallTransport::Xml {
            harness_prompt.push_str("\n\n");
            harness_prompt.push_str(&xml_protocol_prompt(&specs));
        }
        let insertion = request.messages.iter().rposition(|m| m.role == Role::User).unwrap_or(request.messages.len());
        request.messages.insert(insertion, ChatMessage { role: Role::System, content: harness_prompt, ..Default::default() });
        request.tools = if transport == ToolCallTransport::Native { specs } else { Vec::new() };

        let mut total_calls = 0u32;
        let mut previous_signature = String::new();
        let mut identical_rounds = 0u32;
        for round in 1..=settings.max_rounds {
            if stop.is_cancelled() { run_state.status = RunStatus::Stopped; yield HarnessEvent::Stopped { run: run_state.clone() }; return; }
            run_state.round = round;
            yield HarnessEvent::RoundStarted { run: run_state.clone() };
            let mut stream = match provider.stream_chat(request.clone()).await {
                Ok(stream) => stream,
                Err(error) => { yield HarnessEvent::Failed { message: error.to_string() }; return; }
            };
            let mut working = String::new();
            let mut calls = Vec::new();
            loop {
                tokio::select! {
                    biased;
                    _ = stop.cancelled() => { run_state.status = RunStatus::Stopped; yield HarnessEvent::Stopped { run: run_state.clone() }; return; }
                    item = stream.next() => match item {
                        Some(Ok(ModelEvent::TextDelta(text) | ModelEvent::ReasoningDelta(text))) => {
                            if transport == ToolCallTransport::Xml && working.len().saturating_add(text.len()) > crate::agent::MAX_XML_ROUND_BYTES {
                                yield HarnessEvent::Failed { message: "xml_round_too_large".into() }; return;
                            }
                            working.push_str(&text)
                        },
                        Some(Ok(ModelEvent::ToolCall(call))) => calls.push(call),
                        Some(Ok(ModelEvent::Usage { input_tokens, output_tokens })) => yield HarnessEvent::Usage { input_tokens, output_tokens },
                        Some(Ok(ModelEvent::Finished { .. })) => {},
                        Some(Err(error)) => { yield HarnessEvent::Failed { message: error.to_string() }; return; }
                        None => break,
                    }
                }
            }
            if transport == ToolCallTransport::Xml {
                let parsed = match parse_xml_tool_round(&working) {
                    Ok(parsed) => parsed,
                    Err(message) if message == "unterminated_tool_call" => crate::xml_tools::XmlRound { calls: Vec::new(), working_text: working.clone() },
                    Err(message) => { yield HarnessEvent::Failed { message }; return; }
                };
                working = parsed.working_text;
                calls = parsed.calls;
            }
            let mut ids = HashSet::new();
            if calls.iter().any(|call| call.id.trim().is_empty() || !ids.insert(call.id.clone())) {
                yield HarnessEvent::Failed { message: "invalid or duplicate Tool call id".into() };
                return;
            }
            total_calls = total_calls.saturating_add(calls.len() as u32);
            run_state.tool_calls = total_calls;
            if total_calls > settings.max_tool_calls {
                yield HarnessEvent::Failed { message: "configured Tool call limit reached".into() };
                return;
            }
            let signature = call_signature(&calls);
            if !signature.is_empty() && signature == previous_signature { identical_rounds += 1; } else { identical_rounds = 1; }
            previous_signature = signature;
            if !calls.is_empty() && identical_rounds > settings.max_identical_call_rounds {
                yield HarnessEvent::Failed { message: "configured repeated-call limit reached".into() };
                return;
            }

            let mut results = Vec::new();
            let mut finished = None;
            for call in &calls {
                if stop.is_cancelled() { run_state.status = RunStatus::Stopped; yield HarnessEvent::Stopped { run: run_state.clone() }; return; }
                yield HarnessEvent::ToolStarted { call: call.clone() };
                let timed = tokio::time::timeout(std::time::Duration::from_millis(settings.tool_timeout_ms), registry.execute(call, &settings.enabled_tools));
                let execution = tokio::select! {
                    biased;
                    _ = stop.cancelled() => { run_state.status = RunStatus::Stopped; yield HarnessEvent::Stopped { run: run_state.clone() }; return; }
                    result = timed => match result {
                    Ok(execution) => execution,
                    Err(_) => crate::tools::ToolExecution {
                        result: ToolResult { call_id: call.id.clone(), name: call.name.clone(), status: ToolResultStatus::Failed, output: serde_json::json!({}), error_code: Some("timeout".into()) },
                        control: ToolControl::None,
                    },
                    }
                };
                yield HarnessEvent::ToolFinished { result: execution.result.clone() };
                match execution.control {
                    ToolControl::Status { message, user_visible } if user_visible && settings.show_user_status => yield HarnessEvent::Status { message },
                    ToolControl::Finish { response } => { finished = Some(response); results.push(execution.result); break; }
                    _ => results.push(execution.result),
                }
            }
            if let Some(response) = finished {
                run_state.status = RunStatus::Completed;
                yield HarnessEvent::Finished { response, run: run_state.clone() };
                return;
            }

            request.messages.push(ChatMessage { role: Role::Assistant, content: working, tool_calls: calls, ..Default::default() });
            for result in results {
                if transport == ToolCallTransport::Native {
                    request.messages.push(ChatMessage { role: Role::User, tool_result: Some(result), ..Default::default() });
                } else {
                    request.messages.push(ChatMessage { role: Role::User, content: render_xml_tool_result(&result), ..Default::default() });
                }
            }
            request.messages.push(ChatMessage { role: Role::System, content: settings.unfinished_prompt.clone(), ..Default::default() });
        }
        yield HarnessEvent::Failed { message: "configured Agent round limit reached before finish".into() };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use futures::stream::{self, BoxStream};
    use std::collections::VecDeque;
    use std::sync::Mutex;

    struct Scripted(Mutex<VecDeque<Vec<ModelEvent>>>);
    #[async_trait]
    impl ModelProvider for Scripted {
        async fn stream_chat(
            &self,
            _req: ChatRequest,
        ) -> crate::Result<BoxStream<'static, crate::Result<ModelEvent>>> {
            let events = self.0.lock().unwrap().pop_front().unwrap_or_default();
            Ok(Box::pin(stream::iter(events.into_iter().map(Ok))))
        }
    }
    fn request() -> ChatRequest {
        ChatRequest {
            model: "m".into(),
            messages: vec![ChatMessage {
                role: Role::User,
                content: "hello".into(),
                ..Default::default()
            }],
            summary: None,
            max_tokens: None,
            tools: Vec::new(),
        }
    }
    fn finish_call(response: &str) -> ToolCall {
        ToolCall {
            id: "f".into(),
            name: "shirita.run.finish".into(),
            arguments: serde_json::json!({"response": response}),
            transport: ToolCallTransport::Native,
        }
    }

    #[tokio::test]
    async fn ordinary_text_does_not_implicitly_finish() {
        let provider = Arc::new(Scripted(Mutex::new(VecDeque::from([
            vec![ModelEvent::TextDelta("private draft".into())],
            vec![ModelEvent::ToolCall(finish_call("visible"))],
        ]))));
        let events = run(
            provider,
            request(),
            AgentSettings::default(),
            true,
            Arc::new(crate::tools::builtin_tool_registry()),
            StopToken::never(),
            run_state(),
        )
        .collect::<Vec<_>>()
        .await;
        assert!(events.iter().any(|event| matches!(event, HarnessEvent::Finished { response, .. } if response == "visible")));
        assert!(!events.iter().any(|event| matches!(event, HarnessEvent::Finished { response, .. } if response.contains("private"))));
    }

    #[tokio::test]
    async fn xml_finish_uses_the_same_registry_contract() {
        let provider = Arc::new(Scripted(Mutex::new(VecDeque::from([vec![
            ModelEvent::TextDelta(r#"thinking<tool_call id="f" name="shirita.run.finish">{"response":"xml "#.into()),
            ModelEvent::TextDelta(r#"visible"}</tool_call>"#.into()),
        ]]))));
        let mut settings = AgentSettings::default();
        settings.transport = ToolTransport::Xml;
        let events = run(
            provider,
            request(),
            settings,
            false,
            Arc::new(crate::tools::builtin_tool_registry()),
            StopToken::never(),
            run_state(),
        )
        .collect::<Vec<_>>()
        .await;
        assert!(events.iter().any(|event| matches!(event, HarnessEvent::Finished { response, .. } if response == "xml visible")));
    }

    #[tokio::test]
    async fn stop_before_a_round_prevents_the_provider_call() {
        let provider = Arc::new(Scripted(Mutex::new(VecDeque::from([vec![ModelEvent::ToolCall(finish_call("late"))]]))));
        let (handle, token) = crate::conversation::StopHandle::new();
        handle.stop();
        let events = run(provider, request(), AgentSettings::default(), true, Arc::new(crate::tools::builtin_tool_registry()), token, run_state()).collect::<Vec<_>>().await;
        assert!(events.iter().any(|event| matches!(event, HarnessEvent::Stopped { .. })));
    }

    #[tokio::test]
    async fn truncated_xml_gets_an_unfinished_retry() {
        let provider = Arc::new(Scripted(Mutex::new(VecDeque::from([
            vec![ModelEvent::TextDelta(r#"<tool_call id="f" name="shirita.run.finish">{"response":"cut"#.into())],
            vec![ModelEvent::TextDelta(r#"<tool_call id="f2" name="shirita.run.finish">{"response":"recovered"}</tool_call>"#.into())],
        ]))));
        let mut settings = AgentSettings::default(); settings.transport = ToolTransport::Xml;
        let events = run(provider, request(), settings, false, Arc::new(crate::tools::builtin_tool_registry()), StopToken::never(), run_state()).collect::<Vec<_>>().await;
        assert!(events.iter().any(|event| matches!(event, HarnessEvent::Finished { response, .. } if response == "recovered")));
    }

    fn run_state() -> GenerationRun {
        GenerationRun { id: "run".into(), session_id: "session".into(), parent_message_id: None, kind: crate::agent::RunKind::Send, round: 0, tool_calls: 0, status: RunStatus::Running }
    }
}
