use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use serde::Deserialize;

use std::collections::HashMap;

use shirita_core::tree::active_path;
use shirita_core::{Message, OwnerKind, Session};

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
        // Display-side regex is applied at read time (list_messages); store raw only.
        msg.display_content = None;
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

/// Delete a message and its entire subtree (all descendants). If the session's
/// active branch pointed into the deleted subtree, the active leaf is reset to
/// the deleted root's parent (or cleared if the root was top-level). Returns the
/// new active leaf id for the session (may be `null`).
pub async fn delete_message(
    State(state): State<AppState>,
    Path((session_id, msg_id)): Path<(String, String)>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    // Verify the message belongs to this session before deleting (a stale
    // msg_id from another session must not succeed, and the subtree delete
    // would otherwise touch rows in the wrong session's tree).
    let msg = state
        .storage
        .get_message(&msg_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;
    if msg.session_id != session_id {
        return Err(StatusCode::NOT_FOUND);
    }
    let new_leaf = state
        .storage
        .delete_message_subtree(&msg_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(serde_json::json!({ "activeLeafId": new_leaf })))
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

#[derive(Deserialize)]
pub struct ForkBody {
    pub message_id: String,
}

/// Fork: deep-copy the linear path root→`message_id` (current branch) into a new
/// session; carries template/mounts/override_config; `current_state` = the
/// node's snapshot; `active_leaf_id` = the copied leaf. Original untouched.
pub async fn fork_session(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
    Json(body): Json<ForkBody>,
) -> Result<Json<Session>, StatusCode> {
    let src = state
        .storage
        .get_session(&session_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;
    let all = state
        .storage
        .list_messages(&session_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let path = active_path(&all, Some(&body.message_id));
    if path.is_empty() {
        return Err(StatusCode::NOT_FOUND);
    }
    // Preserve all historical turns, not just the active path: include
    // sibling messages (swipes) for every message on the path so the fork
    // captures the full branching history.
    let path_ids: std::collections::HashSet<&str> = path.iter().map(|m| m.id.as_str()).collect();
    let mut slice: Vec<Message> = path.iter().map(|m| (*m).clone()).collect();
    for m in &all {
        if !path_ids.contains(m.id.as_str()) {
            // Include if its parent is on the path (sibling of a path message)
            if let Some(ref pid) = m.parent_id {
                if path_ids.contains(pid.as_str()) {
                    slice.push(m.clone());
                }
            }
        }
    }
    // Sort into a stable order: path messages first, then siblings, both
    // ordered by created_at so parent_id remapping is deterministic.
    slice.sort_by(|a, b| {
        let a_on_path = path_ids.contains(a.id.as_str());
        let b_on_path = path_ids.contains(b.id.as_str());
        if a_on_path == b_on_path {
            a.created_at.cmp(&b.created_at).then_with(|| a.id.cmp(&b.id))
        } else if a_on_path { std::cmp::Ordering::Less } else { std::cmp::Ordering::Greater }
    });
    let node = (*path.last().unwrap()).clone();

    let mut dup = Session::new(format!("{} (fork)", src.name));
    dup.avatar = src.avatar.clone();
    dup.template_id = src.template_id.clone();
    dup.override_config = src.override_config.clone();
    dup.current_state = node.snapshot_state.clone();
    dup.mounted_definitions = src.mounted_definitions.clone();
    state
        .storage
        .create_session(&dup)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let _ = state
        .storage
        .copy_nodes(&OwnerKind::Session, &session_id, &OwnerKind::Session, &dup.id)
        .await;

    // copy the path messages with fresh ids + remapped parents, in one transaction
    let idmap: HashMap<String, String> =
        slice.iter().map(|m| (m.id.clone(), uuid::Uuid::new_v4().to_string())).collect();
    let cloned: Vec<Message> = slice
        .iter()
        .map(|m| {
            let mut nm = (*m).clone();
            nm.id = idmap[&m.id].clone();
            nm.session_id = dup.id.clone();
            nm.parent_id = m.parent_id.as_ref().and_then(|p| idmap.get(p).cloned());
            nm
        })
        .collect();
    let new_leaf = cloned.last().map(|m| m.id.clone());
    state
        .storage
        .create_messages(&cloned)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let _ = state.storage.set_session_active_leaf(&dup.id, new_leaf.as_deref()).await;

    // Copy the rolling digest from the source session and remap the cutoff to the message IDs in the new session (matching the idmap from the deep copy of the messages).
    // The idmap only covers the active path slice (root→message_id); digests after the fork point are omitted (as expected).
    if let Ok(summaries) = state.storage.list_summaries(&session_id).await {
        for s in summaries {
            if let Some(new_cutoff) = idmap.get(&s.cutoff_message_id) {
                let copy = shirita_core::models::summary::Summary::new(&dup.id, new_cutoff, &s.content);
                let _ = state.storage.create_summary(&copy).await;
            }
        }
    }

    let out = state
        .storage
        .get_session(&dup.id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(out))
}
