//! A deterministic mock MCP server over stdio, for tests and manual demos.
//!
//! Reads newline-framed JSON-RPC from stdin and writes responses to stdout.
//! Tools: `add` (succeeds), `boom` (isError), with paginated `tools/list`.

use std::io::{BufRead, Write};

use serde_json::{json, Value};

const PROTOCOL_VERSION: &str = "2025-11-25";

fn main() {
    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout();
    let mut list_calls = 0usize;
    for line in stdin.lock().lines() {
        let line = match line {
            Ok(l) => l,
            Err(_) => break,
        };
        let message: Value = match serde_json::from_str(&line) {
            Ok(v) => v,
            Err(_) => continue,
        };
        let Some(id) = message.get("id").cloned() else {
            continue; // notification: no response
        };
        let method = message.get("method").and_then(Value::as_str).unwrap_or("");
        let result = match method {
            "initialize" => json!({
                "protocolVersion": PROTOCOL_VERSION,
                "capabilities": {"tools": {"listChanged": true}},
                "serverInfo": {"name": "mock", "version": "1"},
            }),
            "tools/list" => {
                list_calls += 1;
                if list_calls == 1 {
                    json!({"tools": [{"name": "add", "description": "Add two numbers", "inputSchema": {"type": "object", "properties": {"a": {"type": "number"}, "b": {"type": "number"}}, "required": ["a", "b"]}}], "nextCursor": "2"})
                } else {
                    json!({"tools": [{"name": "boom", "description": "Always errors", "inputSchema": {"type": "object"}}]})
                }
            }
            "tools/call" => {
                let name = message["params"]["name"].as_str().unwrap_or("");
                if name == "boom" {
                    json!({"content": [{"type": "text", "text": "server failure"}], "isError": true})
                } else {
                    json!({"content": [{"type": "text", "text": "mcp-result"}], "structuredContent": {"ok": true}})
                }
            }
            "ping" => json!({}),
            "shutdown" => {
                let _ = stdout.flush();
                std::process::exit(0);
            }
            _ => json!({}),
        };
        let response = json!({"jsonrpc": "2.0", "id": id, "result": result});
        if writeln!(stdout, "{response}").is_err() || stdout.flush().is_err() {
            break;
        }
    }
}
