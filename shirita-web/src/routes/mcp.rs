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

use shirita_core::mcp::{McpServerConfig, McpServerRecord, McpSession, McpToolDef};

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
