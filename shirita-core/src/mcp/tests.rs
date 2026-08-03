//! Deterministic mock HTTP MCP server (in-process TCP responder) and client
//! tests. The stdio transport is covered by the integration test in
//! `tests/mcp_stdio.rs` against the `mcp_mock_stdio` binary.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use serde_json::{json, Value};

use super::*;

struct MockHttpServer {
    addr: std::net::SocketAddr,
}

async fn start_mock_http() -> MockHttpServer {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let list_calls = Arc::new(AtomicUsize::new(0));
    tokio::spawn(async move {
        loop {
            let Ok((mut sock, _)) = listener.accept().await else {
                break;
            };
            let list_calls = list_calls.clone();
            tokio::spawn(async move {
                let _ = handle_http(&mut sock, &list_calls).await;
            });
        }
    });
    MockHttpServer { addr }
}

async fn handle_http(
    sock: &mut tokio::net::TcpStream,
    list_calls: &AtomicUsize,
) -> std::io::Result<()> {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
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
    let content_length: usize = headers
        .lines()
        .find_map(|l| {
            l.to_ascii_lowercase()
                .strip_prefix("content-length:")
                .and_then(|v| v.trim().parse().ok())
        })
        .unwrap_or(0);
    while buf.len() < header_end + content_length {
        let n = sock.read(&mut tmp).await?;
        if n == 0 {
            break;
        }
        buf.extend_from_slice(&tmp[..n]);
    }
    let body = String::from_utf8_lossy(&buf[header_end..]).to_string();
    let value: Value = match serde_json::from_str(body.trim()) {
        Ok(v) => v,
        Err(_) => return Ok(()),
    };
    let response = match value.get("id") {
        None => json!({}), // notification: no body
        Some(id) => {
            let method = value.get("method").and_then(Value::as_str).unwrap_or("");
            let result = match method {
                "initialize" => json!({
                    "protocolVersion": MCP_PROTOCOL_VERSION,
                    "capabilities": {},
                    "serverInfo": {"name": "http-mock", "version": "1"},
                }),
                "tools/list" => {
                    if list_calls.fetch_add(1, Ordering::SeqCst) == 0 {
                        json!({"tools": [{"name": "http_add", "description": "Add", "inputSchema": {"type": "object"}}], "nextCursor": "2"})
                    } else {
                        json!({"tools": [{"name": "http_second", "description": "Second", "inputSchema": {"type": "object"}}]})
                    }
                }
                "tools/call" => json!({
                    "content": [{"type": "text", "text": "http-result"}],
                    "structuredContent": {"ok": true},
                }),
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
    Ok(())
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
    assert_eq!(tools.len(), 2, "pagination must collect both pages");
    assert_eq!(tools[0].name, "http_add");
    assert_eq!(tools[1].name, "http_second");
    let result = session.call_tool("http_add", json!({"a": 1})).await.unwrap();
    assert_eq!(result.text, "http-result");
    session.shutdown().await.unwrap();
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
