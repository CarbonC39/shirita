//! Minimal MCP (Model Context Protocol) client, Tools only.
//!
//! Targets the stable `2025-11-25` protocol over stdio and Streamable HTTP.
//! Provider-neutral types (`McpServerConfig`, `McpToolDef`, `McpCallResult`,
//! `McpSession`) stay inside this module boundary; the conversation runtime and
//! Tool registry never see JSON-RPC, transport, process, or HTTP-session types.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

pub mod authorization;
pub mod http;
pub mod registry;
pub mod stdio;

#[cfg(test)]
mod tests;

use std::sync::Arc;

use async_trait::async_trait;

use crate::{Error, Result};

/// The stable protocol version this client speaks.
pub const MCP_PROTOCOL_VERSION: &str = "2025-11-25";

/// How long an `ask`-policy Tool call waits for a user decision before
/// expiring (Stop also resolves the wait).
pub const MCP_AUTHORIZATION_TIMEOUT_MS: u64 = 120_000;

// --- memory / resource limits (named, centralized, exposed) -----------------

/// Max bytes of a single Streamable HTTP response body / SSE payload.
pub const MCP_MAX_HTTP_BODY_BYTES: usize = 8 * 1024 * 1024;
/// Max bytes of a single newline-framed stdio JSON-RPC message.
pub const MCP_MAX_STDIO_LINE_BYTES: usize = 8 * 1024 * 1024;
/// Max `tools/list` pages followed (guards a `nextCursor` loop).
pub const MCP_MAX_DISCOVERY_PAGES: u32 = 16;
/// Max Tools discovered per server (bound the frozen registry).
pub const MCP_MAX_TOOLS_PER_SERVER: usize = 256;
/// Max bytes of a discovered Tool name.
pub const MCP_MAX_TOOL_NAME_BYTES: usize = 512;
/// Max bytes of a discovered Tool description.
pub const MCP_MAX_TOOL_DESCRIPTION_BYTES: usize = 16 * 1024;
/// Max bytes of a discovered Tool input schema.
pub const MCP_MAX_TOOL_SCHEMA_BYTES: usize = 64 * 1024;
/// Max bytes of accumulated `tools/call` text content.
pub const MCP_MAX_RESULT_TEXT_BYTES: usize = 64 * 1024;
/// Max stdio args / env entries and HTTP header entries per server.
pub const MCP_MAX_CONFIG_ITEMS: usize = 64;
/// Max bytes of one configured stdio arg / env value or HTTP header value.
pub const MCP_MAX_CONFIG_ITEM_BYTES: usize = 8 * 1024;

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
            McpTransportConfig::Stdio { command, args, env } => {
                if command.trim().is_empty() {
                    return Err("stdio command is required".into());
                }
                if command.len() > MCP_MAX_CONFIG_ITEM_BYTES {
                    return Err("stdio command too large".into());
                }
                if args.len() + env.len() > MCP_MAX_CONFIG_ITEMS {
                    return Err(format!("stdio args/env exceed {MCP_MAX_CONFIG_ITEMS} items"));
                }
                for item in args.iter().chain(env.iter().map(|(k, _)| k)).chain(env.iter().map(|(_, v)| v)) {
                    if item.len() > MCP_MAX_CONFIG_ITEM_BYTES {
                        return Err("stdio arg/env value too large".into());
                    }
                }
            }
            McpTransportConfig::StreamableHttp { url, headers } => {
                http::validate_url(url)?;
                if headers.len() > MCP_MAX_CONFIG_ITEMS {
                    return Err(format!("http headers exceed {MCP_MAX_CONFIG_ITEMS} items"));
                }
                for (name, value) in headers {
                    http::validate_header_name(name).map_err(|e| e.to_string())?;
                    if value.len() > MCP_MAX_CONFIG_ITEM_BYTES {
                        return Err("http header value too large".into());
                    }
                }
            }
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

/// Access policy for a registered MCP Tool.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum McpAccess {
    /// Callable within run limits.
    Allow,
    /// Visible, but a call waits for user authorization.
    Ask,
}

/// Effective per-Tool policy. A registered MCP tool name maps to an access
/// level; tools not listed default to `disabled` (absent from the
/// model-visible registry).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct McpPolicy {
    #[serde(default)]
    pub tools: std::collections::HashMap<String, McpAccess>,
}

impl McpPolicy {
    /// From a settings map (`mcp.policy` key).
    pub fn from_settings_map(map: &std::collections::HashMap<String, Value>) -> McpPolicy {
        map.get("mcp.policy")
            .and_then(|v| serde_json::from_value(v.clone()).ok())
            .unwrap_or_default()
    }

    pub fn to_settings_pair(&self) -> (String, Value) {
        (
            "mcp.policy".into(),
            serde_json::to_value(self).unwrap_or_else(|_| json!({})),
        )
    }

    /// Global policy merged with an optional conversation override (override
    /// wins per tool).
    pub fn effective(
        global: Option<&McpPolicy>,
        conversation_override: Option<&McpPolicy>,
    ) -> McpPolicy {
        let mut tools = global.map(|g| g.tools.clone()).unwrap_or_default();
        if let Some(override_policy) = conversation_override {
            for (name, access) in &override_policy.tools {
                tools.insert(name.clone(), *access);
            }
        }
        McpPolicy { tools }
    }
}

/// Stable registered name for an MCP Tool: `mcp.<server>.<encoded_tool>`.
/// The encoding is deterministic; normalization collisions are rejected at
/// registration (the registry refuses duplicate names).
pub fn mcp_tool_name(server_id: &str, tool_name: &str) -> String {
    format!("mcp.{}.{}", encode_name_part(server_id), encode_name_part(tool_name))
}

fn encode_name_part(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    for c in name.chars() {
        if c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '.' {
            out.push(c);
        } else if c.is_ascii_uppercase() {
            out.push(c.to_ascii_lowercase());
        } else {
            out.push('_');
            for byte in c.to_string().bytes() {
                out.push_str(&format!("{byte:02x}"));
            }
            out.push('_');
        }
    }
    out
}

/// Tool handler that calls back into a connected MCP session. Conversation code
/// never branches on MCP source; this is just another `ToolHandler`.
pub struct McpToolHandler {
    server_id: String,
    tool_name: String,
    access: McpAccess,
    session: Arc<tokio::sync::Mutex<McpSession>>,
    authorization: Arc<crate::mcp::authorization::AuthorizationBroker>,
    run_id: String,
    session_id: String,
    authorization_timeout_ms: u64,
}

#[async_trait]
impl crate::tools::ToolHandler for McpToolHandler {
    async fn execute(&self, call: &crate::tools::ToolCall) -> crate::tools::ToolExecution {
        if self.access == McpAccess::Ask {
            let decision = self
                .authorization
                .request_and_wait(
                    &self.run_id,
                    &self.session_id,
                    &self.server_id,
                    &self.tool_name,
                    &call.id,
                    call.arguments.clone(),
                    self.authorization_timeout_ms,
                )
                .await;
            match decision {
                crate::mcp::authorization::AuthorizationDecision::Approved => {}
                _ => {
                    // Denied or expired (including Stop): a recoverable
                    // rejection; the model may retry within normal limits.
                    return crate::tools::ToolExecution {
                        result: crate::tools::ToolResult {
                            call_id: call.id.clone(),
                            name: call.name.clone(),
                            status: crate::tools::ToolResultStatus::Rejected,
                            output: json!({}),
                            error_code: Some("authorization_denied".into()),
                        },
                        control: crate::tools::ToolControl::None,
                    };
                }
            }
        }
        let result = match self
            .session
            .lock()
            .await
            .call_tool(&self.tool_name, call.arguments.clone())
            .await
        {
            Ok(r) if !r.is_error => crate::tools::ToolResult {
                call_id: call.id.clone(),
                name: call.name.clone(),
                status: crate::tools::ToolResultStatus::Ok,
                output: r.structured.unwrap_or_else(|| json!({"text": r.text})),
                error_code: None,
            },
            Ok(r) => crate::tools::ToolResult {
                call_id: call.id.clone(),
                name: call.name.clone(),
                status: crate::tools::ToolResultStatus::Rejected,
                output: json!({"text": r.text}),
                error_code: Some("execution_failed".into()),
            },
            Err(e) => {
                tracing::warn!(server = %self.server_id, tool = %self.tool_name, error = %e, "mcp tools/call failed");
                crate::tools::ToolResult {
                    call_id: call.id.clone(),
                    name: call.name.clone(),
                    status: crate::tools::ToolResultStatus::Failed,
                    output: json!({}),
                    error_code: Some("provider_unavailable".into()),
                }
            }
        };
        crate::tools::ToolExecution {
            result,
            control: crate::tools::ToolControl::None,
        }
    }
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

    /// List tools with pagination (`cursor`/`nextCursor`), bounded by declared
    /// page/tool/name/description/schema limits and a duplicate-cursor check.
    pub async fn list_tools(&mut self) -> Result<Vec<McpToolDef>> {
        let mut tools = Vec::new();
        let mut cursor: Option<Value> = None;
        let mut pages = 0u32;
        let mut seen_cursors: std::collections::HashSet<String> = std::collections::HashSet::new();
        loop {
            pages += 1;
            if pages > MCP_MAX_DISCOVERY_PAGES {
                return Err(Error::Mcp(format!(
                    "{}: tools/list exceeded {MCP_MAX_DISCOVERY_PAGES} pages",
                    self.server_id
                )));
            }
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
                        .ok_or_else(|| {
                            Error::Mcp(format!("{}: tools/list entry missing name", self.server_id))
                        })?
                        .to_string();
                    if name.len() > MCP_MAX_TOOL_NAME_BYTES {
                        return Err(Error::Mcp(format!(
                            "{}: tool name exceeds {MCP_MAX_TOOL_NAME_BYTES} bytes",
                            self.server_id
                        )));
                    }
                    let description = t
                        .get("description")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .to_string();
                    if description.len() > MCP_MAX_TOOL_DESCRIPTION_BYTES {
                        return Err(Error::Mcp(format!(
                            "{}: tool description exceeds {MCP_MAX_TOOL_DESCRIPTION_BYTES} bytes",
                            self.server_id
                        )));
                    }
                    let input_schema = t
                        .get("inputSchema")
                        .cloned()
                        .unwrap_or_else(|| json!({"type": "object"}));
                    if serde_json::to_vec(&input_schema)
                        .map(|v| v.len())
                        .unwrap_or(usize::MAX)
                        > MCP_MAX_TOOL_SCHEMA_BYTES
                    {
                        return Err(Error::Mcp(format!(
                            "{}: tool schema exceeds {MCP_MAX_TOOL_SCHEMA_BYTES} bytes",
                            self.server_id
                        )));
                    }
                    tools.push(McpToolDef {
                        name,
                        description,
                        input_schema,
                    });
                    if tools.len() > MCP_MAX_TOOLS_PER_SERVER {
                        return Err(Error::Mcp(format!(
                            "{}: exceeded {MCP_MAX_TOOLS_PER_SERVER} tools",
                            self.server_id
                        )));
                    }
                }
            }
            match res.get("nextCursor") {
                Some(c) if !c.is_null() => {
                    let cursor_key = serde_json::to_string(c).unwrap_or_default();
                    if !seen_cursors.insert(cursor_key) {
                        return Err(Error::Mcp(format!(
                            "{}: tools/list cursor loop detected",
                            self.server_id
                        )));
                    }
                    cursor = Some(c.clone());
                }
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
        let mut unsupported: Vec<String> = Vec::new();
        if let Some(arr) = res.get("content").and_then(Value::as_array) {
            for block in arr {
                match block.get("type").and_then(Value::as_str) {
                    Some("text") => {
                        if let Some(t) = block.get("text").and_then(Value::as_str) {
                            if text.len().saturating_add(t.len()) > MCP_MAX_RESULT_TEXT_BYTES {
                                return Err(Error::Mcp(format!(
                                    "{}: tools/call text exceeds {MCP_MAX_RESULT_TEXT_BYTES} bytes",
                                    self.server_id
                                )));
                            }
                            text.push_str(t);
                        }
                    }
                    Some(other) => unsupported.push(other.to_string()),
                    None => unsupported.push("unknown".into()),
                }
            }
        }
        if let Some(s) = res.get("structuredContent") {
            structured = Some(s.clone());
        }
        // Non-text content (images, resources, embedded resources) is surfaced
        // as a bounded safe description instead of being silently dropped.
        if !unsupported.is_empty() {
            let summary = format!("[unsupported content: {}]", unsupported.join(", "));
            if text.is_empty() {
                text = summary;
            } else if text.len().saturating_add(summary.len()) <= MCP_MAX_RESULT_TEXT_BYTES {
                text.push('\n');
                text.push_str(&summary);
            }
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
