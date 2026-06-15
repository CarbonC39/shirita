use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use serde::Deserialize;

use shirita_core::{Message, Session};

use crate::AppState;

#[derive(Deserialize)]
pub struct EditBody {
    pub content: Option<String>,
    pub is_hidden: Option<bool>,
}

/// In-place edit (overwrite `raw_content`, recompute `display_content`) and/or
/// hide toggle. Does not branch (SillyTavern-style edit).
pub async fn edit_message(
    State(state): State<AppState>,
    Path((session_id, msg_id)): Path<(String, String)>,
    Json(body): Json<EditBody>,
) -> Result<Json<Message>, StatusCode> {
    let mut msg = state
        .storage
        .get_message(&msg_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;
    if msg.session_id != session_id {
        return Err(StatusCode::NOT_FOUND);
    }
    if let Some(content) = body.content {
        let rules = state
            .storage
            .list_definitions()
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
            .into_iter()
            .filter(|d| d.def_type == "regex_rule")
            .collect::<Vec<_>>();
        msg.display_content = shirita_core::apply_regex_rules(&content, &rules);
        msg.raw_content = content;
    }
    if let Some(hidden) = body.is_hidden {
        msg.is_hidden = hidden;
    }
    state
        .storage
        .update_message(&msg)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(msg))
}

#[derive(Deserialize)]
pub struct ActiveLeafBody {
    pub message_id: String,
}

/// Move the active branch: descend from `message_id` to its deepest leaf and
/// store that as `active_leaf_id`. Returns the updated session.
pub async fn set_active_leaf(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
    Json(body): Json<ActiveLeafBody>,
) -> Result<Json<Session>, StatusCode> {
    let all = state
        .storage
        .list_messages(&session_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    if !all.iter().any(|m| m.id == body.message_id) {
        return Err(StatusCode::NOT_FOUND);
    }
    let leaf = shirita_core::tree::deepest_leaf(&all, &body.message_id);
    state
        .storage
        .set_session_active_leaf(&session_id, Some(&leaf))
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let session = state
        .storage
        .get_session(&session_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;
    Ok(Json(session))
}
