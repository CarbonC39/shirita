//! Minimal MCP (Model Context Protocol) client, Tools only.
//!
//! Targets the stable `2025-11-25` protocol over stdio and Streamable HTTP.
//! Provider-neutral types (`McpServerConfig`, `McpToolDef`, `McpCallResult`,
//! `McpSession`) stay inside this module boundary; the conversation runtime and
//! Tool registry never see JSON-RPC, transport, process, or HTTP-session types.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

pub mod http;
pub mod stdio;

#[cfg(test)]
mod tests;

use crate::{Error, Result};

/// The stable protocol version this client speaks.
pub const MCP_PROTOCOL_VERSION: &str = "2025-11-25";

/// Transport configuration for one MCP server. Stored typed and revalidated by
/// the runtime at connection time; never trusted from the save route alone.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "transport")]
pub enum McpTransportConfig {
    /// Launch an explicitly configured executable with separate arguments, no
    /// shell, and only the explicit environment entries.
    Stdio {
        command: String,
        args: Vec<String>,
        env: Vec<(String, String)>,
    },
    /// Streamable HTTP endpoint (JSON or SSE responses, session header).
    StreamableHttp {
        url: String,
        /// Static secret headers, validated against the routing/hop-by-hop
        /// denylist; never forwarded from Shirita's own auth.
        headers: Vec<(String, String)>,
    },
}

/// Typed MCP server record (the persisted shape minus stored secrets, which the
/// API never returns).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct McpServerConfig {
    pub id: String,
    pub name: String,
    pub enabled: bool,
    pub transport: McpTransportConfig,
    pub request_timeout_ms: u64,
    /// Whether a secret is present (never the secret itself); derived, so it is
    /// not required on input.
    #[serde(default)]
    pub has_secret: bool,
}

/// A persisted MCP server record: the full typed config (which may contain
/// secret header/env values, never returned by the API) plus timestamps.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct McpServerRecord {
    pub config: McpServerConfig,
    pub created_at: String,
    pub updated_at: String,
}

impl McpTransportConfig {
    /// Whether any header/env value is non-empty (a stored secret is present).
    pub fn has_secrets(&self) -> bool {
        match self {
            McpTransportConfig::Stdio { env, .. } => env.iter().any(|(_, v)| !v.is_empty()),
            McpTransportConfig::StreamableHttp { headers, .. } => {
                headers.iter().any(|(_, v)| !v.is_empty())
            }
        }
    }

    /// A redacted copy: header/env values are blanked so secrets never leave
    /// the backend. Returns whether any value was redacted.
    pub fn redacted(&self) -> (McpTransportConfig, bool) {
        match self {
            McpTransportConfig::Stdio { command, args, env } => {
                let has = env.iter().any(|(_, v)| !v.is_empty());
                let env = env.iter().map(|(k, _)| (k.clone(), String::new())).collect();
                (
                    McpTransportConfig::Stdio {
                        command: command.clone(),
                        args: args.clone(),
                        env,
                    },
                    has,
                )
            }
            McpTransportConfig::StreamableHttp { url, headers } => {
                let has = headers.iter().any(|(_, v)| !v.is_empty());
                let headers = headers.iter().map(|(k, _)| (k.clone(), String::new())).collect();
                (
                    McpTransportConfig::StreamableHttp {
                        url: url.clone(),
                        headers,
                    },
                    has,
                )
            }
        }
    }
}

impl McpServerConfig {
    /// An API-facing copy with secrets blanked and `has_secret` computed.
    pub fn redacted(&self) -> McpServerConfig {
        let (transport, has) = self.transport.redacted();
        McpServerConfig {
            transport,
            has_secret: has,
            ..self.clone()
        }
    }

    /// Merge an incoming (possibly redacted) config over the stored one: an
    /// empty header/env value keeps the stored secret for that key.
    pub fn merge_secrets(&self, incoming: &McpServerConfig) -> McpServerConfig {
        let transport = match &incoming.transport {
            McpTransportConfig::Stdio {
                command,
                args,
                env,
            } => {
                let stored = match &self.transport {
                    McpTransportConfig::Stdio { env, .. } => env.clone(),
                    _ => Vec::new(),
                };
                McpTransportConfig::Stdio {
                    command: command.clone(),
                    args: args.clone(),
                    env: merge_pairs(&stored, env),
                }
            }
            McpTransportConfig::StreamableHttp { url, headers } => {
                let stored = match &self.transport {
                    McpTransportConfig::StreamableHttp { headers, .. } => headers.clone(),
                    _ => Vec::new(),
                };
                McpTransportConfig::StreamableHttp {
                    url: url.clone(),
                    headers: merge_pairs(&stored, headers),
                }
            }
        };
        let has_secret = transport.has_secrets();
        McpServerConfig {
            id: incoming.id.clone(),
            name: incoming.name.clone(),
            enabled: incoming.enabled,
            transport,
            request_timeout_ms: incoming.request_timeout_ms,
            has_secret,
        }
    }

    /// Configuration validation at save time (connection-time revalidation is
    /// performed separately by `McpSession::connect`).
    pub fn validate(&self) -> std::result::Result<(), String> {
        if self.id.is_empty() || !self.id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.') {
            return Err("mcp server id must be alphanumeric, '-', '_', or '.'".into());
        }
        if self.name.trim().is_empty() {
            return Err("mcp server name is required".into());
        }
        if self.request_timeout_ms == 0 || self.request_timeout_ms > 300_000 {
            return Err("mcp request timeout must be between 1 and 300000 ms".into());
        }
        match &self.transport {
            McpTransportConfig::Stdio { command, .. } => {
                if command.trim().is_empty() {
                    return Err("stdio command is required".into());
                }
            }
            McpTransportConfig::StreamableHttp { url, .. } => http::validate_url(url)?,
        }
        Ok(())
    }
}

fn merge_pairs(stored: &[(String, String)], incoming: &[(String, String)]) -> Vec<(String, String)> {
    incoming
        .iter()
        .map(|(key, value)| {
            if value.is_empty() {
                let stored_value = stored
                    .iter()
                    .find(|(stored_key, _)| stored_key == key)
                    .map(|(_, sv)| sv.clone())
                    .unwrap_or_default();
                (key.clone(), stored_value)
            } else {
                (key.clone(), value.clone())
            }
        })
        .collect()
}

/// A normalized Tool from `tools/list`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct McpToolDef {
    pub name: String,
    pub description: String,
    pub input_schema: Value,
}

/// Normalized result of `tools/call`.
#[derive(Debug, Clone, PartialEq)]
pub struct McpCallResult {
    pub is_error: bool,
    /// Concatenated text content blocks, bounded.
    pub text: String,
    /// `structuredContent` when present.
    pub structured: Option<Value>,
}

/// An established MCP session (after `initialize` + `notifications/initialized`).
pub struct McpSession {
    server_id: String,
    inner: McpInner,
    next_id: u64,
    timeout_ms: u64,
}

enum McpInner {
    Stdio(stdio::StdioSession),
    Http(http::HttpSession),
}

impl McpSession {
    /// Connect, negotiate the protocol version, and send `notifications/initialized`.
    pub async fn connect(config: &McpServerConfig) -> Result<McpSession> {
        let timeout_ms = config.request_timeout_ms.max(1);
        let inner = match &config.transport {
            McpTransportConfig::Stdio { command, args, env } => McpInner::Stdio(
                stdio::StdioSession::spawn(command, args, env, timeout_ms).await?,
            ),
            McpTransportConfig::StreamableHttp { url, headers } => McpInner::Http(
                http::HttpSession::new(url, headers, timeout_ms).await?,
            ),
        };
        let mut session = McpSession {
            server_id: config.id.clone(),
            inner,
            next_id: 0,
            timeout_ms,
        };
        session.initialize().await?;
        Ok(session)
    }

    pub fn server_id(&self) -> &str {
        &self.server_id
    }

    async fn request(&mut self, method: &str, params: Value) -> Result<Value> {
        self.next_id = self.next_id.saturating_add(1);
        let id = self.next_id;
        let message = json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params});
        let fut = async {
            match &mut self.inner {
                McpInner::Stdio(s) => s.request(id, &message).await,
                McpInner::Http(s) => s.request(id, &message).await,
            }
        };
        tokio::time::timeout(std::time::Duration::from_millis(self.timeout_ms), fut)
            .await
            .map_err(|_| Error::Mcp(format!("{}: request timed out", self.server_id)))?
    }

    async fn notify(&self, method: &str, params: Value) -> Result<()> {
        let message = json!({"jsonrpc": "2.0", "method": method, "params": params});
        match &self.inner {
            McpInner::Stdio(s) => s.notify(&message).await,
            McpInner::Http(s) => s.notify(&message).await,
        }
    }

    async fn initialize(&mut self) -> Result<()> {
        let params = json!({
            "protocolVersion": MCP_PROTOCOL_VERSION,
            "capabilities": {},
            "clientInfo": {"name": "shirita", "version": env!("CARGO_PKG_VERSION")},
        });
        let _ = self.request("initialize", params).await?;
        self.notify("notifications/initialized", json!({})).await?;
        Ok(())
    }

    /// List tools with pagination (`cursor`/`nextCursor`).
    pub async fn list_tools(&mut self) -> Result<Vec<McpToolDef>> {
        let mut tools = Vec::new();
        let mut cursor: Option<Value> = None;
        loop {
            let params = match &cursor {
                Some(c) => json!({"cursor": c}),
                None => json!({}),
            };
            let res = self.request("tools/list", params).await?;
            if let Some(arr) = res.get("tools").and_then(Value::as_array) {
                for t in arr {
                    let name = t
                        .get("name")
                        .and_then(Value::as_str)
                        .ok_or_else(|| Error::Mcp(format!("{}: tools/list entry missing name", self.server_id)))?
                        .to_string();
                    let description = t
                        .get("description")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .to_string();
                    let input_schema = t
                        .get("inputSchema")
                        .cloned()
                        .unwrap_or_else(|| json!({"type": "object"}));
                    tools.push(McpToolDef {
                        name,
                        description,
                        input_schema,
                    });
                }
            }
            match res.get("nextCursor") {
                Some(c) if !c.is_null() => cursor = Some(c.clone()),
                _ => break,
            }
        }
        Ok(tools)
    }

    /// Call a tool and normalize the result. Non-text content blocks are ignored
    /// (bounded text + `structuredContent` are the model-visible contract).
    pub async fn call_tool(&mut self, name: &str, arguments: Value) -> Result<McpCallResult> {
        let params = json!({"name": name, "arguments": arguments});
        let res = self.request("tools/call", params).await?;
        let is_error = res.get("isError").and_then(Value::as_bool).unwrap_or(false);
        let mut text = String::new();
        let mut structured = None;
        if let Some(arr) = res.get("content").and_then(Value::as_array) {
            for block in arr {
                if block.get("type").and_then(Value::as_str) == Some("text") {
                    if let Some(t) = block.get("text").and_then(Value::as_str) {
                        text.push_str(t);
                    }
                }
            }
        }
        if let Some(s) = res.get("structuredContent") {
            structured = Some(s.clone());
        }
        Ok(McpCallResult {
            is_error,
            text,
            structured,
        })
    }

    pub async fn ping(&mut self) -> Result<()> {
        let _ = self.request("ping", json!({})).await?;
        Ok(())
    }

    /// Graceful shutdown: request `shutdown`, then close the transport.
    pub async fn shutdown(mut self) -> Result<()> {
        let _ = self.request("shutdown", json!({})).await;
        match &mut self.inner {
            McpInner::Stdio(s) => s.close().await,
            McpInner::Http(_) => Ok(()),
        }
    }
}

/// Extract the JSON-RPC `result` from a message, or surface the `error`.
pub(crate) fn parse_result(message: &Value) -> Result<Value> {
    if let Some(error) = message.get("error").filter(|e| !e.is_null()) {
        let code = error.get("code").and_then(Value::as_i64).unwrap_or(-1);
        let text = error
            .get("message")
            .and_then(Value::as_str)
            .unwrap_or("json-rpc error");
        return Err(Error::Mcp(format!("json-rpc error {code}: {text}")));
    }
    message
        .get("result")
        .cloned()
        .ok_or_else(|| Error::Mcp("json-rpc response missing result".into()))
}
