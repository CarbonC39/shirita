//! Integration test: the stdio transport against the deterministic mock MCP
//! server binary (`mcp_mock_stdio`).

use serde_json::json;

use shirita_core::mcp::{
    McpServerConfig, McpSession, McpTransportConfig,
};

#[tokio::test]
async fn stdio_transport_lists_tools_with_pagination_and_calls() {
    let config = McpServerConfig {
        id: "mock".into(),
        name: "stdio mock".into(),
        enabled: true,
        transport: McpTransportConfig::Stdio {
            command: env!("CARGO_BIN_EXE_mcp_mock_stdio").to_string(),
            args: vec![],
            env: vec![],
        },
        request_timeout_ms: 5000,
        has_secret: false,
    };

    let mut session = McpSession::connect(&config)
        .await
        .unwrap_or_else(|e| panic!("stdio connect failed: {e}"));

    let tools = session.list_tools().await.unwrap();
    assert_eq!(tools.len(), 2, "pagination must collect both pages");
    assert_eq!(tools[0].name, "add");
    assert_eq!(tools[1].name, "boom");

    let result = session.call_tool("add", json!({"a": 1, "b": 2})).await.unwrap();
    assert_eq!(result.text, "mcp-result");
    assert_eq!(result.structured, Some(json!({"ok": true})));
    assert!(!result.is_error);

    let failed = session.call_tool("boom", json!({})).await.unwrap();
    assert!(failed.is_error);
    assert_eq!(failed.text, "server failure");

    session.shutdown().await.unwrap();
}

#[tokio::test]
async fn stdio_missing_binary_fails_connect_with_a_clear_error() {
    let config = McpServerConfig {
        id: "missing".into(),
        name: "missing".into(),
        enabled: true,
        transport: McpTransportConfig::Stdio {
            command: "/nonexistent/mcp-server".into(),
            args: vec![],
            env: vec![],
        },
        request_timeout_ms: 1000,
        has_secret: false,
    };
    let err = McpSession::connect(&config).await.err().unwrap();
    assert!(err.to_string().contains("failed to launch"));
}
