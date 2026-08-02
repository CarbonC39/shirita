use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use serde::Serialize;

use shirita_core::agent::{
    AgentSettings, HARD_MAX_ROUNDS, HARD_MAX_TOOL_CALLS, HARD_MAX_TOOL_TIMEOUT_MS,
    MAX_AGENT_PROMPT_BYTES, MAX_MATH_EXPRESSION_BYTES, MAX_MATH_PARSE_DEPTH,
    MAX_RANDOM_INTEGER_SPAN, MAX_RANDOM_ITEMS, MAX_RESPONSE_PATCH_OPS,
    MAX_RESPONSE_PATCH_REPLACE_BYTES, MAX_RESPONSE_PATCH_SEARCH_BYTES,
    MAX_RESPONSE_WORKSPACE_BYTES, MAX_STATUS_MESSAGE_BYTES, MAX_TOOL_ARGUMENT_BYTES,
    MAX_TOOL_RESULT_BYTES, MAX_XML_ROUND_BYTES,
};
use shirita_core::{builtin_tool_registry, ToolSpec};

use crate::AppState;

#[derive(Serialize)]
pub struct AgentLimits {
    hard_max_rounds: u32,
    hard_max_tool_calls: u32,
    hard_max_tool_timeout_ms: u64,
    max_tool_argument_bytes: usize,
    max_tool_result_bytes: usize,
    max_finish_response_bytes: usize,
    max_xml_round_bytes: usize,
    max_agent_prompt_bytes: usize,
    max_status_message_bytes: usize,
    max_random_items: usize,
    max_random_integer_span: i128,
    max_math_expression_bytes: usize,
    max_math_parse_depth: usize,
    max_response_workspace_bytes: usize,
    max_response_patch_ops: usize,
    max_response_patch_search_bytes: usize,
    max_response_patch_replace_bytes: usize,
}

#[derive(Serialize)]
pub struct AgentSettingsView {
    global: AgentSettings,
    #[serde(rename = "override")]
    session_override: Option<AgentSettings>,
    effective: AgentSettings,
    limits: AgentLimits,
    tools: Vec<ToolSpec>,
}

fn registry_data() -> (Vec<String>, Vec<ToolSpec>) {
    let registry = builtin_tool_registry();
    (registry.capability_names(), registry.specs())
}

fn limits() -> AgentLimits {
    AgentLimits {
        hard_max_rounds: HARD_MAX_ROUNDS,
        hard_max_tool_calls: HARD_MAX_TOOL_CALLS,
        hard_max_tool_timeout_ms: HARD_MAX_TOOL_TIMEOUT_MS,
        max_tool_argument_bytes: MAX_TOOL_ARGUMENT_BYTES,
        max_tool_result_bytes: MAX_TOOL_RESULT_BYTES,
        // One-shot finish(response) atomically replaces the workspace, so its
        // ceiling is the response-workspace ceiling, not the legacy 1 MiB bound.
        max_finish_response_bytes: MAX_RESPONSE_WORKSPACE_BYTES,
        max_xml_round_bytes: MAX_XML_ROUND_BYTES,
        max_agent_prompt_bytes: MAX_AGENT_PROMPT_BYTES,
        max_status_message_bytes: MAX_STATUS_MESSAGE_BYTES,
        max_random_items: MAX_RANDOM_ITEMS,
        max_random_integer_span: MAX_RANDOM_INTEGER_SPAN,
        max_math_expression_bytes: MAX_MATH_EXPRESSION_BYTES,
        max_math_parse_depth: MAX_MATH_PARSE_DEPTH,
        max_response_workspace_bytes: MAX_RESPONSE_WORKSPACE_BYTES,
        max_response_patch_ops: MAX_RESPONSE_PATCH_OPS,
        max_response_patch_search_bytes: MAX_RESPONSE_PATCH_SEARCH_BYTES,
        max_response_patch_replace_bytes: MAX_RESPONSE_PATCH_REPLACE_BYTES,
    }
}

async fn global_settings(state: &AppState) -> Result<AgentSettings, StatusCode> {
    let pairs = state
        .storage
        .list_settings()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let settings = AgentSettings::from_settings_map(&pairs.into_iter().collect());
    let (capabilities, _) = registry_data();
    settings
        .validate(&capabilities)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(settings)
}

fn validate(settings: &AgentSettings) -> Result<(), StatusCode> {
    let (capabilities, _) = registry_data();
    settings
        .validate(&capabilities)
        .map_err(|_| StatusCode::BAD_REQUEST)
}

fn view(global: AgentSettings, session_override: Option<AgentSettings>) -> AgentSettingsView {
    let effective = session_override.clone().unwrap_or_else(|| global.clone());
    let (_, tools) = registry_data();
    AgentSettingsView {
        global,
        session_override,
        effective,
        limits: limits(),
        tools,
    }
}

pub async fn get_global(
    State(state): State<AppState>,
) -> Result<Json<AgentSettingsView>, StatusCode> {
    let global = global_settings(&state).await?;
    Ok(Json(view(global, None)))
}

pub async fn put_global(
    State(state): State<AppState>,
    Json(settings): Json<AgentSettings>,
) -> Result<Json<AgentSettingsView>, StatusCode> {
    validate(&settings)?;
    let pairs: Vec<_> = settings.to_settings_map().into_iter().collect();
    state
        .storage
        .set_settings(&pairs)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(view(settings, None)))
}

pub async fn reset_global(
    State(state): State<AppState>,
) -> Result<Json<AgentSettingsView>, StatusCode> {
    let keys: Vec<_> = AgentSettings::default()
        .to_settings_map()
        .into_iter()
        .map(|(key, _)| key)
        .collect();
    state
        .storage
        .delete_settings(&keys)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let global = AgentSettings::default();
    Ok(Json(view(global, None)))
}

pub async fn get_session(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<AgentSettingsView>, StatusCode> {
    let global = global_settings(&state).await?;
    let session = state
        .storage
        .get_session(&id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;
    let session_override = match session.override_config.get("agent") {
        Some(value) => Some(
            serde_json::from_value(value.clone()).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?,
        ),
        None => None,
    };
    Ok(Json(view(global, session_override)))
}

pub async fn put_session(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(settings): Json<AgentSettings>,
) -> Result<Json<AgentSettingsView>, StatusCode> {
    validate(&settings)?;
    if state
        .storage
        .get_session(&id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .is_none()
    {
        return Err(StatusCode::NOT_FOUND);
    }
    let value = serde_json::to_value(&settings).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    state
        .storage
        .set_session_agent_override(&id, &value)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let global = global_settings(&state).await?;
    Ok(Json(view(global, Some(settings))))
}

pub async fn reset_session(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<AgentSettingsView>, StatusCode> {
    if state
        .storage
        .get_session(&id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .is_none()
    {
        return Err(StatusCode::NOT_FOUND);
    }
    state
        .storage
        .clear_session_agent_override(&id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let global = global_settings(&state).await?;
    Ok(Json(view(global, None)))
}
