//! Integration test: policy-gated MCP Tools in the frozen run registry, with
//! `allow` executing through the mock stdio server and `ask` returning
//! `authorization_denied`.

use std::collections::HashMap;

use shirita_core::mcp::registry::build_effective_tool_registry;
use shirita_core::mcp::{McpAccess, McpPolicy, McpServerConfig, McpServerRecord, McpTransportConfig};
use shirita_core::mcp::mcp_tool_name;
use shirita_core::models::session::Session;
use shirita_core::storage::Storage;
use shirita_core::tools::{ToolCall, ToolCallTransport, ToolResultStatus};
use shirita_core::SqliteStorage;

#[tokio::test]
async fn policy_gates_mcp_tools_and_allow_executes_through_the_mock() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("mcp_registry.db");
    let storage = SqliteStorage::connect(path.to_str().unwrap())
        .await
        .unwrap();
    storage.run_migrations().await.unwrap();

    let server = McpServerRecord {
        config: McpServerConfig {
            id: "mock".into(),
            name: "mock".into(),
            enabled: true,
            transport: McpTransportConfig::Stdio {
                command: env!("CARGO_BIN_EXE_mcp_mock_stdio").to_string(),
                args: vec![],
                env: vec![],
            },
            request_timeout_ms: 5000,
            has_secret: false,
        },
        created_at: "t0".into(),
        updated_at: "t0".into(),
    };
    storage.create_mcp_server(&server).await.unwrap();

    // Global policy: add -> allow, boom -> ask; everything else disabled.
    let policy = McpPolicy {
        tools: HashMap::from([
            (mcp_tool_name("mock", "add"), McpAccess::Allow),
            (mcp_tool_name("mock", "boom"), McpAccess::Ask),
        ]),
    };
    storage
        .set_setting("mcp.policy", &serde_json::to_value(&policy).unwrap())
        .await
        .unwrap();

    let session = Session::new("registry-session");
    storage.create_session(&session).await.unwrap();

    let registry = build_effective_tool_registry(&storage, &session)
        .await
        .unwrap();
    let add_name = mcp_tool_name("mock", "add");
    let boom_name = mcp_tool_name("mock", "boom");

    // Policy-allowed tools are registered and model-visible; an unpoliced tool
    // would be absent (the mock only exposes add and boom).
    assert!(registry.spec(&add_name).is_some());
    assert!(registry.spec(&boom_name).is_some());
    assert!(registry.spec(&add_name).unwrap().selected);
    assert!(!registry.spec(&add_name).unwrap().required);

    // allow executes through the mock server.
    let call = ToolCall {
        id: "c1".into(),
        name: add_name.clone(),
        arguments: serde_json::json!({"a": 1, "b": 2}),
        transport: ToolCallTransport::Native,
    };
    let exec = registry.execute(&call, &[]).await;
    assert_eq!(exec.result.status, ToolResultStatus::Ok);
    assert_eq!(exec.result.output["ok"], true);

    // ask returns authorization_denied (the approval flow is a later step).
    let ask_call = ToolCall {
        id: "c2".into(),
        name: boom_name,
        arguments: serde_json::json!({}),
        transport: ToolCallTransport::Native,
    };
    let exec = registry.execute(&ask_call, &[]).await;
    assert_eq!(exec.result.error_code.as_deref(), Some("authorization_denied"));
}

#[tokio::test]
async fn no_enabled_servers_returns_builtins_without_connecting() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("mcp_empty.db");
    let storage = SqliteStorage::connect(path.to_str().unwrap())
        .await
        .unwrap();
    storage.run_migrations().await.unwrap();
    let session = Session::new("empty");
    storage.create_session(&session).await.unwrap();

    let registry = build_effective_tool_registry(&storage, &session)
        .await
        .unwrap();
    assert!(registry.spec("shirita.run.finish").is_some());
    assert!(registry.spec("mcp.none.tool").is_none());
}
