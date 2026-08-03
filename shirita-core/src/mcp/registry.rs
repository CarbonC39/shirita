//! Building the frozen run registry: builtin harness/capability Tools plus
//! policy-allowed MCP Tools, with stable names and collision rejection.

use std::collections::HashMap;
use std::sync::Arc;

use serde_json::Value;

use crate::models::session::Session;
use crate::storage::Storage;
use crate::tools::{ToolRegistry, ToolSpec, ToolSource};
use crate::Result;

use super::authorization::AuthorizationBroker;
use super::{
    McpAccess, McpPolicy, McpSession, McpToolHandler, mcp_tool_name,
    MCP_AUTHORIZATION_TIMEOUT_MS, MCP_MAX_EFFECTIVE_TOOLS, MCP_MAX_ENABLED_SERVERS_PER_RUN,
};

/// Read the effective MCP policy: the global `mcp.policy` setting, optionally
/// overridden per conversation by `session.override_config.mcp_policy`.
pub async fn effective_mcp_policy(storage: &dyn Storage, session: &Session) -> McpPolicy {
    let map: HashMap<String, Value> = storage
        .list_settings()
        .await
        .unwrap_or_default()
        .into_iter()
        .collect();
    let global = McpPolicy::from_settings_map(&map);
    let override_policy = session
        .override_config
        .get("mcp_policy")
        .and_then(|v| serde_json::from_value(v.clone()).ok());
    McpPolicy::effective(Some(&global), override_policy.as_ref())
}

/// Build the frozen registry for one run. When no enabled MCP server exists this
/// returns the builtin registry without any connection. Connect/discovery
/// failures for an enabled server are best-effort: that server's Tools are
/// skipped with a warning rather than failing the whole generation.
pub async fn build_effective_tool_registry(
    storage: &dyn Storage,
    session: &Session,
    authorization: Arc<AuthorizationBroker>,
    run_id: &str,
) -> Result<Arc<ToolRegistry>> {
    let servers = storage.list_mcp_servers().await?;
    if !servers.iter().any(|s| s.config.enabled) {
        return Ok(Arc::new(crate::tools::builtin_tool_registry()));
    }
    let policy = effective_mcp_policy(storage, session).await;
    let mut builder = crate::tools::builtin_tool_registry_builder();
    let mut enabled_servers = 0usize;
    let mut effective_tools = 0usize;
    for server in servers.iter().filter(|s| s.config.enabled) {
        if enabled_servers >= MCP_MAX_ENABLED_SERVERS_PER_RUN {
            tracing::warn!(
                server = %server.config.id,
                limit = MCP_MAX_ENABLED_SERVERS_PER_RUN,
                "skipping MCP server: enabled-server limit reached"
            );
            continue;
        }
        if effective_tools >= MCP_MAX_EFFECTIVE_TOOLS {
            tracing::warn!(
                limit = MCP_MAX_EFFECTIVE_TOOLS,
                "skipping remaining MCP servers: effective Tool limit reached"
            );
            break;
        }
        enabled_servers += 1;
        let session_handle = match McpSession::connect(&server.config).await {
            Ok(session) => Arc::new(tokio::sync::Mutex::new(session)),
            Err(e) => {
                tracing::warn!(server = %server.config.id, error = %e, "mcp server unavailable; skipping its tools");
                continue;
            }
        };
        let tools = match session_handle.lock().await.list_tools().await {
            Ok(tools) => tools,
            Err(e) => {
                tracing::warn!(server = %server.config.id, error = %e, "mcp tools/list failed; skipping server tools");
                continue;
            }
        };
        for tool in tools {
            if effective_tools >= MCP_MAX_EFFECTIVE_TOOLS {
                tracing::warn!(
                    limit = MCP_MAX_EFFECTIVE_TOOLS,
                    server = %server.config.id,
                    "stopping Tool registration: effective Tool limit reached"
                );
                break;
            }
            let name = mcp_tool_name(&server.config.id, &tool.name);
            let Some(access) = policy.tools.get(&name) else {
                continue; // default policy: disabled -> absent from the registry
            };
            let spec = ToolSpec {
                name: name.clone(),
                description: tool.description.clone(),
                input_schema: tool.input_schema.clone(),
                output_schema: None,
                source: ToolSource::Mcp,
                required: false,
                selected: true,
                requires_authorization: *access == McpAccess::Ask,
            };
            let handler = McpToolHandler {
                server_id: server.config.id.clone(),
                tool_name: tool.name.clone(),
                access: *access,
                session: session_handle.clone(),
                authorization: authorization.clone(),
                run_id: run_id.to_string(),
                session_id: session.id.clone(),
                authorization_timeout_ms: MCP_AUTHORIZATION_TIMEOUT_MS,
            };
            if let Err(e) = builder.register_owned(spec, Arc::new(handler)) {
                // Collision or malformed name: never overwrite an existing handler.
                tracing::warn!(server = %server.config.id, tool = %tool.name, error = %e, "mcp tool registration rejected");
            } else {
                effective_tools += 1;
            }
        }
    }
    Ok(Arc::new(builder.build()))
}
