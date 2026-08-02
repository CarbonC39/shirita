//! Provider-neutral multi-round RP Agent execution.

use std::collections::HashSet;
use std::sync::Arc;

use async_stream::stream;
use futures::{Stream, StreamExt};

use crate::agent::{AgentSettings, GenerationRun, ResponseWorkspace, RunStatus, ToolTransport};
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
    /// A successful workspace mutation bumped the revision.
    WorkspaceChanged { revision: u64 },
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
            // Exactly one canonical workspace snapshot is overlaid onto a clone
            // for this round; the run context itself never accumulates drafts.
            let mut round_request = request.clone();
            inject_workspace_snapshot(&mut round_request, &run_state.workspace, round == 1);
            let mut stream = match provider.stream_chat(round_request).await {
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

            let mut results: Vec<ToolResult> = Vec::new();
            let mut pending_finish: Option<(ToolCall, Option<String>)> = None;
            let mut preceding_failed = false;
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
                match execution.control {
                    ToolControl::Status { message, user_visible } if user_visible && settings.show_user_status => {
                        yield HarnessEvent::ToolFinished { result: execution.result.clone() };
                        yield HarnessEvent::Status { message };
                        preceding_failed |= execution.result.status != ToolResultStatus::Ok;
                        results.push(execution.result);
                    }
                    ToolControl::Status { .. } => {
                        yield HarnessEvent::ToolFinished { result: execution.result.clone() };
                        preceding_failed |= execution.result.status != ToolResultStatus::Ok;
                        results.push(execution.result);
                    }
                    ToolControl::ReplaceResponse { text } => {
                        let result = apply_replace(&mut run_state.workspace, call, text);
                        preceding_failed |= result.status != ToolResultStatus::Ok;
                        let ok = result.status == ToolResultStatus::Ok;
                        yield HarnessEvent::ToolFinished { result: result.clone() };
                        if ok {
                            yield HarnessEvent::WorkspaceChanged { revision: run_state.workspace.revision };
                        }
                        results.push(result);
                    }
                    ToolControl::PatchResponse { revision, operations } => {
                        let result = apply_patch(&mut run_state.workspace, call, revision, operations);
                        preceding_failed |= result.status != ToolResultStatus::Ok;
                        let ok = result.status == ToolResultStatus::Ok;
                        yield HarnessEvent::ToolFinished { result: result.clone() };
                        if ok {
                            yield HarnessEvent::WorkspaceChanged { revision: run_state.workspace.revision };
                        }
                        results.push(result);
                    }
                    ToolControl::Finish { response } => {
                        if preceding_failed {
                            // The fence still holds: finish is not executed, but
                            // later calls are never run either. Report it.
                            let skipped = crate::tools::rejected(call, "finish_skipped");
                            yield HarnessEvent::ToolFinished { result: skipped.result.clone() };
                            results.push(skipped.result);
                        } else {
                            pending_finish = Some((call.clone(), response));
                            yield HarnessEvent::ToolFinished { result: execution.result.clone() };
                        }
                        break;
                    }
                    ToolControl::None => {
                        yield HarnessEvent::ToolFinished { result: execution.result.clone() };
                        preceding_failed |= execution.result.status != ToolResultStatus::Ok;
                        results.push(execution.result);
                    }
                }
            }
            if let Some((finish_call, response)) = pending_finish {
                match commit_finish(&mut run_state.workspace, response) {
                    Ok(committed) => {
                        run_state.status = RunStatus::Completed;
                        yield HarnessEvent::Finished { response: committed, run: run_state.clone() };
                        return;
                    }
                    Err(code) => {
                        // Recoverable invalid finish (e.g. empty workspace):
                        // the fence still held, but the run continues.
                        let rejected = crate::tools::rejected(&finish_call, code);
                        yield HarnessEvent::ToolFinished { result: rejected.result.clone() };
                        results.push(rejected.result);
                    }
                }
            }

            append_round(&mut request, &working, &calls, &results, transport, &settings.unfinished_prompt);
        }
        yield HarnessEvent::Failed { message: "configured Agent round limit reached before finish".into() };
    }
}

fn apply_replace(workspace: &mut ResponseWorkspace, call: &ToolCall, text: String) -> ToolResult {
    workspace.text = text;
    workspace.revision = workspace.revision.saturating_add(1);
    ToolResult {
        call_id: call.id.clone(),
        name: call.name.clone(),
        status: ToolResultStatus::Ok,
        output: serde_json::json!({ "revision": workspace.revision }),
        error_code: None,
    }
}

fn apply_patch(
    workspace: &mut ResponseWorkspace,
    call: &ToolCall,
    revision: u64,
    operations: Vec<crate::agent::ResponsePatchOperation>,
) -> ToolResult {
    if revision != workspace.revision {
        return crate::tools::rejected(call, "stale_revision").result;
    }
    let mut candidate = workspace.text.clone();
    for op in &operations {
        let matches: Vec<_> = candidate.match_indices(&op.search).collect();
        if matches.len() != 1 {
            let code = if matches.is_empty() {
                "search_not_found"
            } else {
                "search_not_unique"
            };
            return crate::tools::rejected(call, code).result;
        }
        let (at, _) = matches[0];
        candidate.replace_range(at..at + op.search.len(), &op.replace);
        if candidate.len() > crate::agent::MAX_RESPONSE_WORKSPACE_BYTES {
            return crate::tools::rejected(call, "response_too_large").result;
        }
    }
    workspace.text = candidate;
    workspace.revision = workspace.revision.saturating_add(1);
    ToolResult {
        call_id: call.id.clone(),
        name: call.name.clone(),
        status: ToolResultStatus::Ok,
        output: serde_json::json!({ "revision": workspace.revision, "operations": operations.len() }),
        error_code: None,
    }
}

fn commit_finish(
    workspace: &mut ResponseWorkspace,
    response: Option<String>,
) -> Result<String, &'static str> {
    match response {
        Some(text) => {
            workspace.text = text;
            workspace.revision = workspace.revision.saturating_add(1);
            Ok(workspace.text.clone())
        }
        None if workspace.text.trim().is_empty() => Err("empty_response"),
        None => Ok(workspace.text.clone()),
    }
}

fn render_workspace_snapshot(workspace: &ResponseWorkspace) -> String {
    let payload = serde_json::json!({
        "revision": workspace.revision,
        "state": if workspace.text.is_empty() { "empty" } else { "present" },
        "text": workspace.text,
    });
    format!(
        "SHIRITA_RESPONSE_WORKSPACE\n{}\nEND_SHIRITA_RESPONSE_WORKSPACE",
        crate::xml_tools::safe_json(&payload)
    )
}

/// Overlay exactly one canonical snapshot onto a clone of the run context.
/// Round 1 places it before the current user turn; later rounds after retained
/// Tool results and immediately before the configured unfinished instruction.
fn inject_workspace_snapshot(
    request: &mut ChatRequest,
    workspace: &ResponseWorkspace,
    first_round: bool,
) {
    let snapshot = render_workspace_snapshot(workspace);
    let message = ChatMessage {
        role: Role::System,
        content: snapshot,
        ..Default::default()
    };
    if first_round {
        let insertion = request
            .messages
            .iter()
            .rposition(|m| m.role == Role::User)
            .unwrap_or(request.messages.len());
        request.messages.insert(insertion, message);
    } else {
        request.messages.insert(request.messages.len().saturating_sub(1), message);
    }
}

fn render_receipt(call: &ToolCall, result: &ToolResult) -> String {
    let revision = result.output["revision"].as_u64().unwrap_or(0);
    if call.name == "shirita.response.replace" {
        format!("response replaced -> revision {revision}")
    } else {
        let ops = result.output["operations"].as_u64().unwrap_or(0);
        format!("response patched ({ops} operations) -> revision {revision}")
    }
}

/// Append the round's retained Tool conversation. Successful response-control
/// call/result pairs are compacted into bounded receipts; the canonical
/// snapshot is an overlay and is never stored in the run context.
fn append_round(
    request: &mut ChatRequest,
    working: &str,
    calls: &[ToolCall],
    results: &[ToolResult],
    transport: ToolCallTransport,
    unfinished: &str,
) {
    let mut retained_calls = Vec::new();
    let mut retained_results = Vec::new();
    let mut receipts = Vec::new();
    for (call, result) in calls.iter().zip(results.iter()) {
        if crate::tools::is_response_control(&call.name) && result.status == ToolResultStatus::Ok {
            receipts.push(render_receipt(call, result));
        } else {
            retained_calls.push(call.clone());
            retained_results.push(result.clone());
        }
    }
    if !working.is_empty() || !retained_calls.is_empty() {
        request.messages.push(ChatMessage {
            role: Role::Assistant,
            content: working.to_string(),
            tool_calls: retained_calls,
            ..Default::default()
        });
    }
    for result in retained_results {
        if transport == ToolCallTransport::Native {
            request.messages.push(ChatMessage {
                role: Role::User,
                tool_result: Some(result),
                ..Default::default()
            });
        } else {
            request.messages.push(ChatMessage {
                role: Role::User,
                content: render_xml_tool_result(&result),
                ..Default::default()
            });
        }
    }
    for receipt in receipts {
        request.messages.push(ChatMessage {
            role: Role::System,
            content: receipt,
            ..Default::default()
        });
    }
    request.messages.push(ChatMessage {
        role: Role::System,
        content: unfinished.to_string(),
        ..Default::default()
    });
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
    async fn stop_during_provider_streaming_aborts_the_run() {
        struct Infinite;
        #[async_trait]
        impl ModelProvider for Infinite {
            async fn stream_chat(
                &self,
                _req: ChatRequest,
            ) -> crate::Result<BoxStream<'static, crate::Result<ModelEvent>>> {
                // Usage events are surfaced to the caller, so the test can fire
                // Stop once a few have arrived; plain text is private working
                // output and is never yielded, so it could not drive the timer.
                Ok(Box::pin(futures::stream::repeat_with(|| {
                    Ok(ModelEvent::Usage {
                        input_tokens: 1,
                        output_tokens: 1,
                    })
                })))
            }
        }
        let (handle, token) = crate::conversation::StopHandle::new();
        let stream = run(
            Arc::new(Infinite),
            request(),
            AgentSettings::default(),
            true,
            Arc::new(crate::tools::builtin_tool_registry()),
            token,
            run_state(),
        );
        let events = async move {
            let mut events = Vec::new();
            futures::pin_mut!(stream);
            while let Some(event) = stream.next().await {
                events.push(event);
                if events.len() >= 3 {
                    handle.stop();
                }
            }
            events
        };
        let events = events.await;
        assert!(events.iter().any(|e| matches!(e, HarnessEvent::Stopped { .. })));
    }

    struct SlowTool;
    #[async_trait]
    impl crate::tools::ToolHandler for SlowTool {
        async fn execute(&self, _call: &ToolCall) -> crate::tools::ToolExecution {
            std::future::pending::<()>().await;
            unreachable!()
        }
    }
    fn slow_registry() -> Arc<ToolRegistry> {
        Arc::new(
            crate::tools::ToolRegistry::builder()
                .register(
                    crate::tools::ToolSpec {
                        name: "slow.tool".into(),
                        description: String::new(),
                        input_schema: serde_json::json!({}),
                        output_schema: None,
                        source: crate::tools::ToolSource::Builtin,
                        required: false,
                    },
                    Arc::new(SlowTool),
                )
                .unwrap()
                .build(),
        )
    }

    #[tokio::test]
    async fn stop_during_tool_execution_is_cooperative() {
        let provider = Arc::new(Scripted(Mutex::new(VecDeque::from([vec![
            ModelEvent::ToolCall(ToolCall {
                id: "s".into(),
                name: "slow.tool".into(),
                arguments: serde_json::json!({}),
                transport: ToolCallTransport::Native,
            }),
        ]]))));
        let mut settings = AgentSettings::default();
        settings.enabled_tools = vec!["slow.tool".into()];
        let (handle, token) = crate::conversation::StopHandle::new();
        let stream = run(
            provider,
            request(),
            settings,
            true,
            slow_registry(),
            token,
            run_state(),
        );
        let events = async move {
            let mut events = Vec::new();
            futures::pin_mut!(stream);
            while let Some(event) = stream.next().await {
                events.push(event);
                if events.iter().any(|e| matches!(e, HarnessEvent::ToolStarted { .. })) {
                    handle.stop();
                }
            }
            events
        };
        let events = events.await;
        assert!(events.iter().any(|e| matches!(e, HarnessEvent::Stopped { .. })));
    }

    #[tokio::test]
    async fn stop_between_rounds_prevents_the_next_round() {
        let (provider, seen) = recording(vec![vec![ModelEvent::ToolCall(replace_call("hi"))]]);
        let (handle, token) = crate::conversation::StopHandle::new();
        let stream = run(
            provider,
            request(),
            AgentSettings::default(),
            true,
            Arc::new(crate::tools::builtin_tool_registry()),
            token,
            run_state(),
        );
        let events = async move {
            let mut events = Vec::new();
            futures::pin_mut!(stream);
            while let Some(event) = stream.next().await {
                events.push(event);
                if events.iter().any(|e| matches!(e, HarnessEvent::WorkspaceChanged { .. })) {
                    handle.stop();
                }
            }
            events
        };
        let events = events.await;
        assert!(events.iter().any(|e| matches!(e, HarnessEvent::Stopped { .. })));
        assert_eq!(seen.lock().unwrap().len(), 1);
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
        GenerationRun::new("run", "session", None, crate::agent::RunKind::Send)
    }

    fn replace_call(text: &str) -> ToolCall {
        ToolCall {
            id: "r".into(),
            name: "shirita.response.replace".into(),
            arguments: serde_json::json!({"text": text}),
            transport: ToolCallTransport::Native,
        }
    }
    fn patch_call(revision: u64, ops: serde_json::Value) -> ToolCall {
        ToolCall {
            id: "p".into(),
            name: "shirita.response.patch".into(),
            arguments: serde_json::json!({"revision": revision, "operations": ops}),
            transport: ToolCallTransport::Native,
        }
    }
    fn finish_call_workspace() -> ToolCall {
        ToolCall {
            id: "f".into(),
            name: "shirita.run.finish".into(),
            arguments: serde_json::json!({}),
            transport: ToolCallTransport::Native,
        }
    }

    /// A provider that also records every request it sees, for transcript
    /// placement assertions.
    struct Recording(
        Mutex<VecDeque<Vec<ModelEvent>>>,
        Arc<Mutex<Vec<ChatRequest>>>,
    );
    #[async_trait]
    impl ModelProvider for Recording {
        async fn stream_chat(
            &self,
            req: ChatRequest,
        ) -> crate::Result<BoxStream<'static, crate::Result<ModelEvent>>> {
            self.1.lock().unwrap().push(req.clone());
            let events = self.0.lock().unwrap().pop_front().unwrap_or_default();
            Ok(Box::pin(stream::iter(events.into_iter().map(Ok))))
        }
    }
    fn recording(rounds: Vec<Vec<ModelEvent>>) -> (Arc<Recording>, Arc<Mutex<Vec<ChatRequest>>>) {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let provider = Arc::new(Recording(Mutex::new(VecDeque::from(rounds)), seen.clone()));
        (provider, seen)
    }
    fn snapshot_messages(req: &ChatRequest) -> Vec<&ChatMessage> {
        req.messages
            .iter()
            .filter(|m| m.content.starts_with("SHIRITA_RESPONSE_WORKSPACE"))
            .collect()
    }

    #[tokio::test]
    async fn replace_then_finish_commits_the_workspace() {
        let provider = Arc::new(Scripted(Mutex::new(VecDeque::from([
            vec![ModelEvent::ToolCall(replace_call("Draft one"))],
            vec![ModelEvent::ToolCall(finish_call_workspace())],
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
        assert!(events
            .iter()
            .any(|e| matches!(e, HarnessEvent::Finished { response, .. } if response == "Draft one")));
        assert!(events
            .iter()
            .any(|e| matches!(e, HarnessEvent::WorkspaceChanged { revision: 1 })));
    }

    #[tokio::test]
    async fn patches_apply_incrementally_and_bump_revision() {
        let provider = Arc::new(Scripted(Mutex::new(VecDeque::from([
            vec![ModelEvent::ToolCall(replace_call("red door; red cloak"))],
            vec![ModelEvent::ToolCall(patch_call(
                1,
                serde_json::json!([
                    {"search": "red door", "replace": "blue door"},
                    {"search": "red", "replace": "black"}
                ]),
            ))],
            vec![ModelEvent::ToolCall(finish_call_workspace())],
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
        assert!(events.iter().any(|e| matches!(
            e,
            HarnessEvent::Finished { response, .. } if response == "blue door; black cloak"
        )));
        let revisions: Vec<_> = events
            .iter()
            .filter_map(|e| match e {
                HarnessEvent::WorkspaceChanged { revision } => Some(*revision),
                _ => None,
            })
            .collect();
        assert_eq!(revisions, vec![1, 2]);
    }

    #[tokio::test]
    async fn stale_revision_patch_is_rejected_without_mutation() {
        let provider = Arc::new(Scripted(Mutex::new(VecDeque::from([
            vec![ModelEvent::ToolCall(replace_call("original"))],
            vec![ModelEvent::ToolCall(patch_call(
                9,
                serde_json::json!([{"search": "orig", "replace": "changed"}]),
            ))],
            vec![ModelEvent::ToolCall(finish_call_workspace())],
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
        assert!(events.iter().any(|e| matches!(
            e,
            HarnessEvent::ToolFinished { result } if result.error_code.as_deref() == Some("stale_revision")
        )));
        assert!(events.iter().any(|e| matches!(
            e,
            HarnessEvent::Finished { response, .. } if response == "original"
        )));
    }

    #[tokio::test]
    async fn ambiguous_patch_rolls_back_atomically() {
        let provider = Arc::new(Scripted(Mutex::new(VecDeque::from([
            vec![ModelEvent::ToolCall(replace_call("alpha beta beta"))],
            vec![ModelEvent::ToolCall(patch_call(
                1,
                serde_json::json!([
                    {"search": "alpha", "replace": "beta"},
                    {"search": "beta", "replace": "gamma"}
                ]),
            ))],
            vec![ModelEvent::ToolCall(finish_call_workspace())],
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
        assert!(events.iter().any(|e| matches!(
            e,
            HarnessEvent::ToolFinished { result } if result.error_code.as_deref() == Some("search_not_unique")
        )));
        assert!(events.iter().any(|e| matches!(
            e,
            HarnessEvent::Finished { response, .. } if response == "alpha beta beta"
        )));
    }

    #[tokio::test]
    async fn finish_with_empty_workspace_is_recoverable() {
        let provider = Arc::new(Scripted(Mutex::new(VecDeque::from([
            vec![ModelEvent::ToolCall(finish_call_workspace())],
            vec![ModelEvent::ToolCall(finish_call("ok"))],
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
        assert!(events.iter().any(|e| matches!(
            e,
            HarnessEvent::ToolFinished { result } if result.error_code.as_deref() == Some("empty_response")
        )));
        assert!(events.iter().any(|e| matches!(
            e,
            HarnessEvent::Finished { response, .. } if response == "ok"
        )));
    }

    #[tokio::test]
    async fn failed_call_before_finish_skips_the_commit() {
        let provider = Arc::new(Scripted(Mutex::new(VecDeque::from([
            vec![
                ModelEvent::ToolCall(ToolCall {
                    id: "m".into(),
                    name: "shirita.math.evaluate".into(),
                    arguments: serde_json::json!({"expression": "system(1)"}),
                    transport: ToolCallTransport::Native,
                }),
                ModelEvent::ToolCall(finish_call_workspace()),
            ],
            vec![ModelEvent::ToolCall(finish_call("recovered"))],
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
        assert!(events.iter().any(|e| matches!(
            e,
            HarnessEvent::ToolFinished { result } if result.error_code.as_deref() == Some("finish_skipped")
        )));
        assert!(events.iter().any(|e| matches!(
            e,
            HarnessEvent::Finished { response, .. } if response == "recovered"
        )));
    }

    #[tokio::test]
    async fn round1_snapshot_is_empty_and_before_the_user_turn() {
        let (provider, seen) = recording(vec![vec![ModelEvent::ToolCall(finish_call("hi"))]]);
        run(
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
        let seen = seen.lock().unwrap();
        assert_eq!(seen.len(), 1);
        let req = &seen[0];
        let snapshots = snapshot_messages(req);
        assert_eq!(snapshots.len(), 1);
        assert!(snapshots[0].content.contains("\"revision\":0"));
        assert!(snapshots[0].content.contains("\"state\":\"empty\""));
        let snapshot_idx = req.messages.iter().position(|m| m.content.starts_with("SHIRITA_RESPONSE_WORKSPACE")).unwrap();
        let user_idx = req.messages.iter().rposition(|m| m.role == Role::User).unwrap();
        assert!(snapshot_idx < user_idx);
    }

    #[tokio::test]
    async fn round2_snapshot_is_current_and_compacts_response_history() {
        let (provider, seen) = recording(vec![
            vec![ModelEvent::ToolCall(replace_call("hi"))],
            vec![ModelEvent::ToolCall(finish_call_workspace())],
        ]);
        run(
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
        let seen = seen.lock().unwrap();
        assert_eq!(seen.len(), 2);
        let req2 = &seen[1];
        // exactly one canonical snapshot, at the current revision
        let snapshots = snapshot_messages(req2);
        assert_eq!(snapshots.len(), 1);
        assert!(snapshots[0].content.contains("\"revision\":1"));
        assert!(snapshots[0].content.contains("\"state\":\"present\""));
        // immediately before the last (unfinished) message
        let snapshot_idx = req2.messages.iter().position(|m| m.content.starts_with("SHIRITA_RESPONSE_WORKSPACE")).unwrap();
        assert_eq!(snapshot_idx + 1, req2.messages.len() - 1);
        assert!(req2.messages.last().unwrap().content.contains("Continue working"));
        // the response-control call/result pair is compacted: no assistant
        // tool_calls, no tool_result, and a bounded receipt instead
        assert!(!req2.messages.iter().any(|m| m.role == Role::Assistant && !m.tool_calls.is_empty()));
        assert!(!req2.messages.iter().any(|m| m.tool_result.is_some()));
        assert!(req2.messages.iter().any(|m| m.content.contains("response replaced -> revision 1")));
    }

    #[tokio::test]
    async fn xml_response_tools_finish_through_the_same_registry() {
        let provider = Arc::new(Scripted(Mutex::new(VecDeque::from([
            vec![ModelEvent::TextDelta(
                r#"<tool_call id="r" name="shirita.response.replace">{"text":"xml draft"}</tool_call>"#.into(),
            )],
            vec![ModelEvent::TextDelta(
                r#"<tool_call id="f" name="shirita.run.finish">{}</tool_call>"#.into(),
            )],
        ]))));
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
        assert!(events.iter().any(|e| matches!(
            e,
            HarnessEvent::Finished { response, .. } if response == "xml draft"
        )));
    }

    #[tokio::test]
    async fn calls_after_the_finish_fence_are_never_executed() {
        let provider = Arc::new(Scripted(Mutex::new(VecDeque::from([
            vec![
                ModelEvent::ToolCall(replace_call("hi")),
                ModelEvent::ToolCall(finish_call_workspace()),
                ModelEvent::ToolCall(ToolCall {
                    id: "late".into(),
                    name: "shirita.random.number".into(),
                    arguments: serde_json::json!({"min": 1, "max": 2}),
                    transport: ToolCallTransport::Native,
                }),
            ],
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
        assert!(events.iter().any(|e| matches!(
            e,
            HarnessEvent::Finished { response, .. } if response == "hi"
        )));
        assert!(!events.iter().any(|e| matches!(
            e,
            HarnessEvent::ToolStarted { call } if call.name == "shirita.random.number"
        )));
    }

    #[tokio::test]
    async fn duplicate_finish_calls_in_a_round_are_ignored() {
        let provider = Arc::new(Scripted(Mutex::new(VecDeque::from([
            vec![
                ModelEvent::ToolCall(replace_call("x")),
                ModelEvent::ToolCall(finish_call_workspace()),
                ModelEvent::ToolCall(ToolCall {
                    id: "f2".into(),
                    name: "shirita.run.finish".into(),
                    arguments: serde_json::json!({"response": "ignored"}),
                    transport: ToolCallTransport::Native,
                }),
            ],
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
        assert!(events.iter().any(|e| matches!(
            e,
            HarnessEvent::Finished { response, .. } if response == "x"
        )));
        assert!(!events.iter().any(|e| matches!(
            e,
            HarnessEvent::Finished { response, .. } if response == "ignored"
        )));
    }

    #[tokio::test]
    async fn capability_results_are_visible_only_in_the_next_round() {
        let (provider, seen) = recording(vec![
            vec![ModelEvent::ToolCall(ToolCall {
                id: "m".into(),
                name: "shirita.math.evaluate".into(),
                arguments: serde_json::json!({"expression": "1+2"}),
                transport: ToolCallTransport::Native,
            })],
            vec![ModelEvent::ToolCall(finish_call("done"))],
        ]);
        run(
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
        let seen = seen.lock().unwrap();
        assert_eq!(seen.len(), 2);
        let req2 = &seen[1];
        // The capability call/result is retained (not compacted) and present in
        // the round-2 request: results become model-visible only next round.
        assert!(req2
            .messages
            .iter()
            .any(|m| m.tool_result.as_ref().is_some_and(|r| r.name == "shirita.math.evaluate")));
        assert!(req2
            .messages
            .iter()
            .any(|m| m.role == Role::Assistant && !m.tool_calls.is_empty()));
    }
}
