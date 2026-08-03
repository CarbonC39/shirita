//! MCP server CRUD, connection test, and Tool discovery API.
//!
//! Stored secrets are never returned: every response carries a redacted config
//! with `has_secret`. Updates merge blank header/env values back onto the stored
//! secret. Connection-time validation is re-done by `McpSession::connect`, not
//! trusted from the save route.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use serde::Serialize;
use serde_json::{json, Value};

use shirita_core::mcp::authorization::{
    AuthorizationDecision, PendingAuthorizationInfo,
};
use shirita_core::mcp::{
    McpServerConfig, McpServerRecord, McpSession, McpToolDef, MCP_AUTHORIZATION_TIMEOUT_MS,
    MCP_MAX_CONFIGURED_SERVERS, MCP_MAX_CONFIG_ITEM_BYTES, MCP_MAX_CONFIG_ITEMS,
    MCP_MAX_DISCOVERY_PAGES, MCP_MAX_EFFECTIVE_TOOLS, MCP_MAX_ENABLED_SERVERS_PER_RUN,
    MCP_MAX_HTTP_BODY_BYTES, MCP_MAX_RESULT_TEXT_BYTES, MCP_MAX_STDIO_LINE_BYTES,
    MCP_MAX_TOOL_DESCRIPTION_BYTES, MCP_MAX_TOOL_NAME_BYTES, MCP_MAX_TOOLS_PER_SERVER,
    MCP_MAX_TOOL_SCHEMA_BYTES,
};

use crate::AppState;

#[derive(Serialize)]
pub struct McpServerView {
    #[serde(flatten)]
    config: McpServerConfig,
    created_at: String,
    updated_at: String,
}

fn view(record: &McpServerRecord) -> McpServerView {
    McpServerView {
        config: record.config.redacted(),
        created_at: record.created_at.clone(),
        updated_at: record.updated_at.clone(),
    }
}

fn now() -> String {
    chrono::Utc::now().to_rfc3339()
}

/// Declared MCP limits, exposed for the UI / operator documentation.
#[derive(Serialize)]
pub struct McpLimits {
    authorization_timeout_ms: u64,
    max_http_body_bytes: usize,
    max_stdio_line_bytes: usize,
    max_discovery_pages: u32,
    max_tools_per_server: usize,
    max_tool_name_bytes: usize,
    max_tool_description_bytes: usize,
    max_tool_schema_bytes: usize,
    max_result_text_bytes: usize,
    max_config_items: usize,
    max_config_item_bytes: usize,
    max_configured_servers: usize,
    max_enabled_servers_per_run: usize,
    max_effective_tools: usize,
}

pub async fn limits() -> Json<McpLimits> {
    Json(McpLimits {
        authorization_timeout_ms: MCP_AUTHORIZATION_TIMEOUT_MS,
        max_http_body_bytes: MCP_MAX_HTTP_BODY_BYTES,
        max_stdio_line_bytes: MCP_MAX_STDIO_LINE_BYTES,
        max_discovery_pages: MCP_MAX_DISCOVERY_PAGES,
        max_tools_per_server: MCP_MAX_TOOLS_PER_SERVER,
        max_tool_name_bytes: MCP_MAX_TOOL_NAME_BYTES,
        max_tool_description_bytes: MCP_MAX_TOOL_DESCRIPTION_BYTES,
        max_tool_schema_bytes: MCP_MAX_TOOL_SCHEMA_BYTES,
        max_result_text_bytes: MCP_MAX_RESULT_TEXT_BYTES,
        max_config_items: MCP_MAX_CONFIG_ITEMS,
        max_config_item_bytes: MCP_MAX_CONFIG_ITEM_BYTES,
        max_configured_servers: MCP_MAX_CONFIGURED_SERVERS,
        max_enabled_servers_per_run: MCP_MAX_ENABLED_SERVERS_PER_RUN,
        max_effective_tools: MCP_MAX_EFFECTIVE_TOOLS,
    })
}

fn internal<E: std::fmt::Display>(e: E) -> StatusCode {
    tracing::error!(error = %e, "mcp route error");
    StatusCode::INTERNAL_SERVER_ERROR
}

pub async fn list_servers(
    State(state): State<AppState>,
) -> Result<Json<Vec<McpServerView>>, StatusCode> {
    let records = state
        .storage
        .list_mcp_servers()
        .await
        .map_err(internal)?;
    Ok(Json(records.iter().map(view).collect()))
}

pub async fn get_server(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<McpServerView>, StatusCode> {
    let record = state
        .storage
        .get_mcp_server(&id)
        .await
        .map_err(internal)?
        .ok_or(StatusCode::NOT_FOUND)?;
    Ok(Json(view(&record)))
}

pub async fn create_server(
    State(state): State<AppState>,
    Json(config): Json<McpServerConfig>,
) -> Result<Json<McpServerView>, StatusCode> {
    let mut config = config;
    if config.id.trim().is_empty() {
        config.id = format!("mcp-{}", uuid::Uuid::new_v4().simple());
    }
    config.validate().map_err(|_| StatusCode::BAD_REQUEST)?;
    let count = state.storage.list_mcp_servers().await.map_err(internal)?.len();
    if count >= MCP_MAX_CONFIGURED_SERVERS {
        return Err(StatusCode::BAD_REQUEST);
    }
    let record = McpServerRecord {
        config,
        created_at: now(),
        updated_at: now(),
    };
    state
        .storage
        .create_mcp_server(&record)
        .await
        .map_err(internal)?;
    Ok(Json(view(&record)))
}

pub async fn update_server(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(config): Json<McpServerConfig>,
) -> Result<Json<McpServerView>, StatusCode> {
    let stored = state
        .storage
        .get_mcp_server(&id)
        .await
        .map_err(internal)?
        .ok_or(StatusCode::NOT_FOUND)?;
    let merged = stored.config.merge_secrets(&config);
    merged.validate().map_err(|_| StatusCode::BAD_REQUEST)?;
    let record = McpServerRecord {
        config: merged,
        created_at: stored.created_at.clone(),
        updated_at: now(),
    };
    state
        .storage
        .update_mcp_server(&record)
        .await
        .map_err(internal)?;
    Ok(Json(view(&record)))
}

pub async fn delete_server(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<StatusCode, StatusCode> {
    state
        .storage
        .delete_mcp_server(&id)
        .await
        .map_err(internal)?;
    Ok(StatusCode::NO_CONTENT)
}

/// Connect, negotiate, and ping. Returns `{ ok, error }` (200 even on failure so
/// the UI can show the message).
pub async fn test_server(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Value>, StatusCode> {
    let record = state
        .storage
        .get_mcp_server(&id)
        .await
        .map_err(internal)?
        .ok_or(StatusCode::NOT_FOUND)?;
    match McpSession::connect(&record.config).await {
        Ok(mut session) => {
            let ping_ok = session.ping().await.is_ok();
            let _ = session.shutdown().await;
            Ok(Json(json!({"ok": ping_ok, "error": null})))
        }
        Err(e) => Ok(Json(json!({"ok": false, "error": e.to_string()}))),
    }
}

/// Connect, discover Tools (paginated), and return them. Never mutates a
/// running run's frozen registry; a later run picks up the refresh.
pub async fn refresh_tools(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Value>, StatusCode> {
    let record = state
        .storage
        .get_mcp_server(&id)
        .await
        .map_err(internal)?
        .ok_or(StatusCode::NOT_FOUND)?;
    match McpSession::connect(&record.config).await {
        Ok(mut session) => {
            let tools: Result<Vec<McpToolDef>, _> = session.list_tools().await;
            let _ = session.shutdown().await;
            match tools {
                Ok(tools) => Ok(Json(json!({"ok": true, "tools": tools}))),
                Err(e) => Ok(Json(json!({"ok": false, "error": e.to_string()}))),
            }
        }
        Err(e) => Ok(Json(json!({"ok": false, "error": e.to_string()}))),
    }
}

/// Approve a pending `ask` Tool call. One-time: a stale or already-decided
/// request returns 404. The frozen arguments recorded at request time are what
/// execute on approval.
pub async fn approve(
    State(state): State<AppState>,
    Path((run_id, call_id)): Path<(String, String)>,
) -> Result<Json<Value>, StatusCode> {
    match state
        .authorization
        .resolve(&run_id, &call_id, AuthorizationDecision::Approved)
        .await
    {
        Some(_) => Ok(Json(json!({"ok": true}))),
        None => Err(StatusCode::NOT_FOUND),
    }
}

pub async fn deny(
    State(state): State<AppState>,
    Path((run_id, call_id)): Path<(String, String)>,
) -> Result<Json<Value>, StatusCode> {
    match state
        .authorization
        .resolve(&run_id, &call_id, AuthorizationDecision::Denied)
        .await
    {
        Some(_) => Ok(Json(json!({"ok": true}))),
        None => Err(StatusCode::NOT_FOUND),
    }
}

/// Pending authorization requests for a session (drives the chat prompt).
pub async fn pending(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
) -> Result<Json<Vec<PendingAuthorizationInfo>>, StatusCode> {
    let pending = state.authorization.pending_for_session(&session_id).await;
    Ok(Json(pending))
}
