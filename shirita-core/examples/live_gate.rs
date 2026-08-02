//! Live Phase 5A acceptance gate against a local OpenAI-compatible server.
//!
//! Drives the real provider + `agent_loop::run` with the built-in registry and
//! records what actually happens, honestly, rather than assuming compliance.
//!
//! Usage:
//! ```text
//! cargo run --release --example live_gate -- http://localhost:8080/v1 gemma-4-e4b
//! ```

use futures::StreamExt;
use std::sync::Arc;

use shirita_core::agent::{AgentSettings, GenerationRun, RunKind, ToolTransport};
use shirita_core::agent_loop::HarnessEvent;
use shirita_core::conversation::StopToken;
use shirita_core::model::{ChatMessage, ChatRequest, ModelProvider, OpenAiProvider};
use shirita_core::models::message::Role;
use shirita_core::tools::builtin_tool_registry;

#[derive(Debug, Clone, Copy, PartialEq)]
enum Outcome {
    Finished,
    NoFinish,
    Failed,
}

async fn scenario(
    provider: Arc<dyn ModelProvider>,
    model: &str,
    transport: ToolTransport,
    instruction: &str,
    label: &str,
) {
    let mut settings = AgentSettings::default();
    settings.enabled = true;
    settings.transport = transport;
    settings.max_rounds = 6;
    settings.max_tool_calls = 16;
    settings.tool_timeout_ms = 30_000;

    let request = ChatRequest {
        model: model.into(),
        messages: vec![ChatMessage {
            role: Role::User,
            content: instruction.into(),
            ..Default::default()
        }],
        summary: None,
        max_tokens: Some(2048),
        tools: Vec::new(),
    };
    let native_supported = transport != ToolTransport::Xml;
    let run_state = GenerationRun::new("live-gate", "session", None, RunKind::Send);

    println!("\n=== {label} (transport={transport:?}) ===");
    let events = shirita_core::agent_loop::run(
        provider.clone(),
        request,
        settings,
        native_supported,
        Arc::new(builtin_tool_registry()),
        StopToken::never(),
        run_state,
    )
    .collect::<Vec<_>>()
    .await;

    let mut outcome = Outcome::NoFinish;
    for event in &events {
        match event {
            HarnessEvent::RoundStarted { run } => {
                println!("  round {} start (workspace rev {})", run.round, run.workspace.revision)
            }
            HarnessEvent::ToolStarted { call } => println!("  tool -> {}", call.name),
            HarnessEvent::ToolFinished { result } => println!(
                "  tool <- {} status={:?} err={:?}",
                result.name, result.status, result.error_code
            ),
            HarnessEvent::WorkspaceChanged { revision } => {
                println!("  workspace changed -> revision {revision}")
            }
            HarnessEvent::Status { message } => println!("  status: {message}"),
            HarnessEvent::Usage { input_tokens, output_tokens } => {
                println!("  usage: {input_tokens} in / {output_tokens} out")
            }
            HarnessEvent::Finished { response, run } => {
                outcome = Outcome::Finished;
                println!("  FINISHED (rev {})", run.workspace.revision);
                println!("  response: {}", truncate(response, 200));
            }
            HarnessEvent::Stopped { .. } => println!("  stopped"),
            HarnessEvent::Failed { message } => {
                outcome = Outcome::Failed;
                println!("  FAILED: {message}")
            }
        }
    }
    println!("  => outcome: {outcome:?}");
}

fn truncate(s: &str, n: usize) -> String {
    if s.len() <= n {
        s.to_string()
    } else {
        format!("{}…", &s[..n])
    }
}

#[tokio::main]
async fn main() {
    let mut args = std::env::args().skip(1);
    let base_url = args.next().unwrap_or_else(|| "http://localhost:8080/v1".into());
    let model = args.next().unwrap_or_else(|| "gemma-4-e4b".into());

    let client = reqwest::Client::new();
    let provider: Arc<dyn ModelProvider> = Arc::new(OpenAiProvider::new(
        client.clone(),
        format!("{base_url}"),
        "local-gate",
    ));

    scenario(
        provider.clone(),
        &model,
        ToolTransport::Native,
        "You are in a text-generation harness. Follow the tool instructions exactly and do not answer the user directly.\n\
         1. Call shirita.math.evaluate with expression \"6*7\".\n\
         2. Wait for its result. Then call shirita.response.replace with text being one short sentence that mentions the computed value.\n\
         3. Then call shirita.run.finish with no arguments to commit the response.",
        "native capability -> edit -> finish",
    )
    .await;

    scenario(
        provider.clone(),
        &model,
        ToolTransport::Native,
        "Use the registered shirita.run.finish tool with the response field set to exactly: The answer is 42. Do not write anything else.",
        "native one-shot finish(response)",
    )
    .await;

    scenario(
        provider.clone(),
        &model,
        ToolTransport::Xml,
        "You are in a text-generation harness using the XML tool protocol. Follow the instructions exactly and do not answer the user directly.\n\
         1. Call the shirita.math.evaluate tool with expression \"5*5\" by emitting one complete <tool_call> block.\n\
         2. Wait for its result. Then call shirita.response.replace with text being one short sentence that mentions the computed value, again as a complete <tool_call> block.\n\
         3. Then call shirita.run.finish with no arguments to commit the response.",
        "xml capability -> edit -> finish",
    )
    .await;

    scenario(
        provider.clone(),
        &model,
        ToolTransport::Native,
        "In a SINGLE reply, call BOTH shirita.math.evaluate with expression \"2+2\" AND shirita.random.choose with items [1,2,3]. Emit both tool calls together.",
        "native multi-tool single round",
    )
    .await;

    scenario(
        provider.clone(),
        &model,
        ToolTransport::Xml,
        "Use the XML tool protocol. Call the shirita.run.finish tool with a complete <tool_call> block whose JSON argument is {\"response\":\"The answer is 42.\"}. Emit only that block.",
        "xml one-shot finish(response)",
    )
    .await;
}
