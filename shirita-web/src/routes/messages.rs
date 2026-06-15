use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use serde::Deserialize;

use shirita_core::Message;

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
