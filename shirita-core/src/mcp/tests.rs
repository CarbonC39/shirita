//! Deterministic mock HTTP MCP server (in-process TCP responder) and client
//! tests. The stdio transport is covered by the integration test in
//! `tests/mcp_stdio.rs` against the `mcp_mock_stdio` binary.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use serde_json::{json, Value};

use super::*;

struct MockHttpServer {
    addr: std::net::SocketAddr,
    loop_cursor: Arc<std::sync::atomic::AtomicBool>,
    tool_count: Arc<AtomicUsize>,
}

async fn start_mock_http() -> MockHttpServer {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let list_calls = Arc::new(AtomicUsize::new(0));
    let loop_cursor = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let tool_count = Arc::new(AtomicUsize::new(2));
    let server_loop_cursor = loop_cursor.clone();
    let server_tool_count = tool_count.clone();
    tokio::spawn(async move {
        loop {
            let Ok((mut sock, _)) = listener.accept().await else {
                break;
            };
            let list_calls = list_calls.clone();
            let loop_cursor = server_loop_cursor.clone();
            let tool_count = server_tool_count.clone();
            let base = format!("http://{addr}");
            tokio::spawn(async move {
                let _ = handle_http(&mut sock, &list_calls, &loop_cursor, &tool_count, &base).await;
            });
        }
    });
    MockHttpServer { addr, loop_cursor, tool_count }
}

async fn handle_http(
    sock: &mut tokio::net::TcpStream,
    list_calls: &AtomicUsize,
    loop_cursor: &std::sync::atomic::AtomicBool,
    tool_count: &AtomicUsize,
    base: &str,
) -> std::io::Result<()> {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    // Handle requests on one connection until the client closes it (so a
    // connection-reusing redirect follow is answered by this same handler).
    loop {
        let mut buf = Vec::new();
        let mut tmp = [0u8; 2048];
        loop {
            let n = sock.read(&mut tmp).await?;
            if n == 0 {
                return Ok(());
            }
            buf.extend_from_slice(&tmp[..n]);
            if buf.windows(4).any(|w| w == b"\r\n\r\n") {
                break;
            }
            if buf.len() > 64 * 1024 {
                return Ok(());
            }
        }
        let header_end = buf
            .windows(4)
            .position(|w| w == b"\r\n\r\n")
            .map(|i| i + 4)
            .unwrap_or(buf.len());
        let headers = String::from_utf8_lossy(&buf[..header_end]).to_string();
    let path = headers
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .unwrap_or("/");
    let content_length: usize = headers
        .lines()
        .find_map(|l| {
            l.to_ascii_lowercase()
                .strip_prefix("content-length:")
                .and_then(|v| v.trim().parse().ok())
        })
        .unwrap_or(0);
    // Drain the full request body before responding so a redirect cannot reset
    // the connection while the client is still writing.
    while buf.len() < header_end + content_length {
        let n = sock.read(&mut tmp).await?;
        if n == 0 {
            break;
        }
        buf.extend_from_slice(&tmp[..n]);
    }
    if path == "/redirect" {
        // A single same-origin redirect to the MCP endpoint.
        let resp = format!("HTTP/1.1 302 Found\r\nLocation: {base}/mcp\r\nContent-Length: 0\r\n\r\n");
        sock.write_all(resp.as_bytes()).await?;
        continue;
    }
    if path == "/loop" {
        // A same-origin redirect loop (client must stop at its count limit).
        let resp = format!("HTTP/1.1 302 Found\r\nLocation: {base}/loop\r\nContent-Length: 0\r\n\r\n");
        sock.write_all(resp.as_bytes()).await?;
        continue;
    }
    let body = String::from_utf8_lossy(&buf[header_end..]).to_string();
    let value: Value = match serde_json::from_str(body.trim()) {
        Ok(v) => v,
        Err(_) => return Ok(()),
    };
    let response = match value.get("id") {
        None => {
            // A notification is acknowledged with an empty 202 body (the client
            // must not require JSON for it).
            let resp = "HTTP/1.1 202 Accepted\r\nContent-Length: 0\r\n\r\n";
            sock.write_all(resp.as_bytes()).await?;
            sock.flush().await?;
            continue;
        }
        Some(id) => {
            let method = value.get("method").and_then(Value::as_str).unwrap_or("");
            let result = match method {
                "initialize" => json!({
                    "protocolVersion": MCP_PROTOCOL_VERSION,
                    "capabilities": {},
                    "serverInfo": {"name": "http-mock", "version": "1"},
                }),
                "tools/list" => {
                    let count = tool_count.load(Ordering::SeqCst);
                    if loop_cursor.load(Ordering::SeqCst) {
                        json!({"tools": [], "nextCursor": "same"})
                    } else if count > 2 {
                        let tools: Vec<Value> = (0..count)
                            .map(|i| json!({"name": format!("bulk_{i}"), "description": "bulk", "inputSchema": {"type": "object"}}))
                            .collect();
                        json!({"tools": tools})
                    } else if list_calls.fetch_add(1, Ordering::SeqCst) == 0 {
                        json!({"tools": [{"name": "http_add", "description": "Add", "inputSchema": {"type": "object"}}, {"name": "http_img", "description": "Image", "inputSchema": {"type": "object"}}], "nextCursor": "2"})
                    } else {
                        json!({"tools": [{"name": "http_second", "description": "Second", "inputSchema": {"type": "object"}}]})
                    }
                }
                "tools/call" => {
                    let name = value["params"]["name"].as_str().unwrap_or("");
                    if name == "http_img" {
                        json!({"content": [{"type": "image", "data": "base64…"}]})
                    } else {
                        json!({
                            "content": [{"type": "text", "text": "http-result"}],
                            "structuredContent": {"ok": true},
                        })
                    }
                }
                "ping" => json!({}),
                _ => json!({}),
            };
            json!({"jsonrpc": "2.0", "id": id, "result": result})
        }
    };
    let resp_body = response.to_string();
    let resp = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
        resp_body.len(),
        resp_body
    );
    sock.write_all(resp.as_bytes()).await?;
    sock.flush().await?;
    }
}

#[tokio::test]
async fn http_lists_tools_with_pagination_and_calls() {
    let server = start_mock_http().await;
    let config = McpServerConfig {
        id: "http-mock".into(),
        name: "HTTP mock".into(),
        enabled: true,
        transport: McpTransportConfig::StreamableHttp {
            url: format!("http://{}/mcp", server.addr),
            headers: vec![("x-api-key".into(), "secret".into())],
        },
        request_timeout_ms: 5000,
        has_secret: true,
    };
    let mut session = McpSession::connect(&config)
        .await
        .unwrap_or_else(|e| panic!("http connect failed: {e}"));
    let tools = session.list_tools().await.unwrap();
    assert_eq!(tools.len(), 3, "pagination must collect both pages");
    assert_eq!(tools[0].name, "http_add");
    assert_eq!(tools[2].name, "http_second");
    let result = session.call_tool("http_add", json!({"a": 1})).await.unwrap();
    assert_eq!(result.text, "http-result");
    // Non-text content is surfaced as a safe description, not silently dropped.
    let image = session.call_tool("http_img", json!({})).await.unwrap();
    assert!(image.text.contains("unsupported content: image"));
    session.shutdown().await.unwrap();
}

#[tokio::test]
async fn http_detects_a_cursor_loop() {
    let server = start_mock_http().await;
    server.loop_cursor.store(true, Ordering::SeqCst);
    let config = McpServerConfig {
        id: "loop".into(),
        name: "loop".into(),
        enabled: true,
        transport: McpTransportConfig::StreamableHttp {
            url: format!("http://{}/mcp", server.addr),
            headers: vec![],
        },
        request_timeout_ms: 5000,
        has_secret: false,
    };
    let mut session = McpSession::connect(&config)
        .await
        .unwrap_or_else(|e| panic!("connect failed: {e}"));
    let err = session.list_tools().await.err().unwrap();
    assert!(err.to_string().contains("cursor loop detected"));
}

#[test]
fn http_rejects_embedded_url_credentials() {
    let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
    rt.block_on(async {
        let config = McpServerConfig {
            id: "bad".into(),
            name: "bad".into(),
            enabled: true,
            transport: McpTransportConfig::StreamableHttp {
                url: "http://user:pass@localhost:9/mcp".into(),
                headers: vec![],
            },
            request_timeout_ms: 1000,
            has_secret: false,
        };
        let err = McpSession::connect(&config).await.err().unwrap();
        assert!(err.to_string().contains("must not embed credentials"));
    });
}

#[test]
fn http_rejects_plain_http_outside_loopback() {
    let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
    rt.block_on(async {
        let err = http::HttpSession::new("http://example.com/mcp", &[], 1000)
            .await
            .err()
            .unwrap();
        assert!(err.to_string().contains("loopback"));
    });
}

#[test]
fn redaction_blanks_secrets_and_merge_keeps_stored_values() {
    let stored = McpServerConfig {
        id: "demo".into(),
        name: "Demo".into(),
        enabled: true,
        transport: McpTransportConfig::StreamableHttp {
            url: "http://localhost:8080/mcp".into(),
            headers: vec![
                ("x-api-key".into(), "s3cret".into()),
                ("X-Tenant".into(), "t1".into()),
            ],
        },
        request_timeout_ms: 5000,
        has_secret: true,
    };
    let redacted = stored.redacted();
    assert!(redacted.has_secret);
    let headers = match &redacted.transport {
        McpTransportConfig::StreamableHttp { headers, .. } => headers,
        _ => panic!("expected http transport"),
    };
    assert_eq!(headers[0].1, "");
    assert_eq!(headers[1].1, "");

    // Merge: an incoming blank value keeps the stored secret; a non-blank value
    // wins.
    let incoming = McpServerConfig {
        id: "demo".into(),
        name: "Demo".into(),
        enabled: true,
        transport: McpTransportConfig::StreamableHttp {
            url: "http://localhost:8080/mcp".into(),
            headers: vec![
                ("x-api-key".into(), "".into()),
                ("X-Tenant".into(), "t2".into()),
            ],
        },
        request_timeout_ms: 5000,
        has_secret: false,
    };
    let merged = stored.merge_secrets(&incoming);
    let headers = match &merged.transport {
        McpTransportConfig::StreamableHttp { headers, .. } => headers,
        _ => panic!("expected http transport"),
    };
    assert_eq!(headers[0].1, "s3cret");
    assert_eq!(headers[1].1, "t2");
}

#[test]
fn preview_redacts_sensitive_values_and_truncates_by_character() {
    use super::authorization::bounded_redacted_preview;
    // A long Chinese value whose byte-512 boundary would land mid-character:
    // the preview must not panic and must still be valid to display.
    let chinese = "中".repeat(300);
    let args = json!({"prompt": chinese, "api_key": "sk-secret", "nested": {"password": "p"}});
    let preview = bounded_redacted_preview(&args);
    assert!(preview.contains("[redacted]"));
    assert!(!preview.contains("sk-secret"));
    assert!(!preview.contains("\"p\""));
    assert!(preview.chars().count() <= 513, "preview must be bounded by characters");
}

#[test]
fn headers_reject_reserved_and_hop_by_hop_names() {
    for bad in ["host", "connection", "content-length", "cookie", "transfer-encoding", "mcp-session-id"] {
        assert!(http::validate_header_name(bad).is_err(), "{bad} must be rejected");
    }
    assert!(http::validate_header_name("x-api-key").is_ok());
}

#[test]
fn validate_rejects_bad_ids_and_remote_plain_http() {
    let mut config = McpServerConfig {
        id: "bad id!".into(),
        name: "Bad".into(),
        enabled: true,
        transport: McpTransportConfig::StreamableHttp {
            url: "http://example.com/mcp".into(),
            headers: vec![],
        },
        request_timeout_ms: 5000,
        has_secret: false,
    };
    assert!(config.validate().is_err());
    config.id = "ok-id".into();
    assert!(config.validate().is_err(), "remote plain http must be rejected");
    config.transport = McpTransportConfig::StreamableHttp {
        url: "http://localhost:8080/mcp".into(),
        headers: vec![],
    };
    assert!(config.validate().is_ok());
}


#[test]
fn redirect_policy_is_same_origin_and_bounded_in_count() {
    use http::should_follow_redirect;
    // original_origin is (scheme, host, port).
    let origin = ("http".to_string(), "localhost".to_string(), Some(8080));
    // Same-origin within the bound -> follow.
    assert!(should_follow_redirect(&origin, "http://localhost:8080/mcp", 0));
    assert!(should_follow_redirect(&origin, "http://localhost:8080/mcp", 2));
    // Same-origin at/over the count bound -> stop (loop fails at the limit).
    assert!(!should_follow_redirect(&origin, "http://localhost:8080/mcp", 3));
    // Cross-origin (different host, port, or scheme) -> stop, never forward
    // configured secret headers.
    assert!(!should_follow_redirect(&origin, "http://evil.example/mcp", 0));
    assert!(!should_follow_redirect(&origin, "http://localhost:9999/mcp", 0));
    assert!(!should_follow_redirect(&origin, "https://localhost:8080/mcp", 0));
}

#[tokio::test]
async fn effective_tool_cap_limits_a_single_server_and_keeps_builtins() {
    use std::collections::HashMap;
    use crate::mcp::registry::build_effective_tool_registry;
    use crate::models::session::Session;
    use crate::mcp::{McpPolicy, McpAccess, McpServerConfig, McpServerRecord, McpTransportConfig, mcp_tool_name};
    use crate::SqliteStorage;

    let server = start_mock_http().await;
    server.tool_count.store(200, Ordering::SeqCst);
    let dir = tempfile::tempdir().unwrap();
    let sqlite = SqliteStorage::connect(dir.path().join("cap.db").to_str().unwrap()).await.unwrap();
    sqlite.run_migrations().await.unwrap();
    let storage: std::sync::Arc<dyn crate::Storage> = std::sync::Arc::new(sqlite);
    storage
        .create_mcp_server(&McpServerRecord {
            config: McpServerConfig {
                id: "bulk".into(),
                name: "bulk".into(),
                enabled: true,
                transport: McpTransportConfig::StreamableHttp {
                    url: format!("http://{}/mcp", server.addr),
                    headers: vec![],
                },
                request_timeout_ms: 5000,
                has_secret: false,
            },
            created_at: "t".into(),
            updated_at: "t".into(),
        })
        .await
        .unwrap();
    // Policy allows all 200 discovered tools.
    let mut tools = HashMap::new();
    for i in 0..200 {
        tools.insert(mcp_tool_name("bulk", &format!("bulk_{i}")), McpAccess::Allow);
    }
    storage
        .set_setting("mcp.policy", &serde_json::to_value(McpPolicy { tools }).unwrap())
        .await
        .unwrap();
    let session = Session::new("cap-session");
    storage.create_session(&session).await.unwrap();

    let registry = build_effective_tool_registry(
        storage.as_ref(),
        &session,
        std::sync::Arc::new(crate::mcp::authorization::AuthorizationBroker::new()),
        "run-cap",
    )
    .await
    .unwrap();
    let mcp_tools = registry.specs().into_iter().filter(|s| s.source == crate::tools::ToolSource::Mcp).count();
    assert_eq!(mcp_tools, 128, "effective MCP Tool cap must bind within a single server");
    // Built-in tools are not counted toward the MCP cap and remain present.
    assert!(registry.spec("shirita.run.finish").is_some());
}

#[tokio::test]
async fn enabled_server_cap_limits_connections_per_run() {
    use std::collections::HashMap;
    use crate::mcp::registry::build_effective_tool_registry;
    use crate::models::session::Session;
    use crate::mcp::{McpPolicy, McpAccess, McpServerConfig, McpServerRecord, McpTransportConfig, mcp_tool_name};
    use crate::SqliteStorage;

    let dir = tempfile::tempdir().unwrap();
    let sqlite = SqliteStorage::connect(dir.path().join("enabled.db").to_str().unwrap()).await.unwrap();
    sqlite.run_migrations().await.unwrap();
    let storage: std::sync::Arc<dyn crate::Storage> = std::sync::Arc::new(sqlite);
    let mut tools = HashMap::new();
    // 9 enabled servers; the per-run enabled-server cap is 8.
    for i in 0..9 {
        let mock = start_mock_http().await;
        storage
            .create_mcp_server(&McpServerRecord {
                config: McpServerConfig {
                    id: format!("srv{i}"),
                    name: format!("srv{i}"),
                    enabled: true,
                    transport: McpTransportConfig::StreamableHttp {
                        url: format!("http://{}/mcp", mock.addr),
                        headers: vec![],
                    },
                    request_timeout_ms: 5000,
                    has_secret: false,
                },
                created_at: "t".into(),
                updated_at: "t".into(),
            })
            .await
            .unwrap();
        tools.insert(mcp_tool_name(&format!("srv{i}"), "http_add"), McpAccess::Allow);
    }
    storage
        .set_setting("mcp.policy", &serde_json::to_value(McpPolicy { tools }).unwrap())
        .await
        .unwrap();
    let session = Session::new("enabled-session");
    storage.create_session(&session).await.unwrap();

    let registry = build_effective_tool_registry(
        storage.as_ref(),
        &session,
        std::sync::Arc::new(crate::mcp::authorization::AuthorizationBroker::new()),
        "run-enabled",
    )
    .await
    .unwrap();
    let mut servers: std::collections::HashSet<String> = std::collections::HashSet::new();
    for spec in registry.specs().into_iter().filter(|s| s.source == crate::tools::ToolSource::Mcp) {
        let prefix = spec.name.split('.').take(2).collect::<Vec<_>>().join(".");
        servers.insert(prefix);
    }
    assert_eq!(servers.len(), 8, "at most 8 enabled servers may be connected per run");
}
