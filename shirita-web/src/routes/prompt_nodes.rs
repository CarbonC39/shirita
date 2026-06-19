use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::Json;
use serde::{Deserialize, Deserializer};

use shirita_core::{NodeKind, OwnerKind, PromptNode};

use crate::AppState;

#[derive(Deserialize)]
pub struct CreateNodeBody { pub parent_id: Option<String>, pub kind: String, pub tag: Option<String>, pub definition_id: Option<String> }

/// 区分「字段未传」(None) vs「字段传了 null」(Some(None)) vs「传了值」(Some(Some(v)))，
/// 否则 update_node 无法把 parent_id/tag/definition_id 显式清空（例如把 ref 移出 folder）。
fn double_option<'de, T, D>(de: D) -> Result<Option<Option<T>>, D::Error>
where
    T: Deserialize<'de>,
    D: Deserializer<'de>,
{
    Deserialize::deserialize(de).map(Some)
}

#[derive(Deserialize)]
pub struct UpdateNodeBody {
    #[serde(default, deserialize_with = "double_option")]
    pub parent_id: Option<Option<String>>,
    pub sort_order: Option<i64>,
    #[serde(default, deserialize_with = "double_option")]
    pub tag: Option<Option<String>>,
    #[serde(default, deserialize_with = "double_option")]
    pub definition_id: Option<Option<String>>,
    pub enabled: Option<bool>,
    pub meta: Option<serde_json::Value>,
}

#[derive(Deserialize)]
pub struct ReorderBody { pub ordered_ids: Vec<String> }

#[derive(Deserialize)]
pub struct NodesQuery { pub owner_kind: String }

/// 强制 2 层树：folder/history 必须挂根（parent 为 None）；ref 的 parent 为 None 或指向
/// 同 owner 集合内一个 folder。违反返回 400。`owner_nodes` 为该 owner 的全部节点。
fn enforce_two_level(kind: &NodeKind, parent_id: &Option<String>, owner_nodes: &[PromptNode]) -> Result<(), StatusCode> {
    match kind {
        NodeKind::Folder | NodeKind::History => {
            if parent_id.is_some() {
                return Err(StatusCode::BAD_REQUEST);
            }
        }
        NodeKind::Ref => {
            if let Some(pid) = parent_id {
                let parent_is_folder = owner_nodes.iter().any(|n| &n.id == pid && n.kind == NodeKind::Folder);
                if !parent_is_folder {
                    return Err(StatusCode::BAD_REQUEST);
                }
            }
        }
    }
    Ok(())
}

pub async fn list_nodes(State(state): State<AppState>, Path(owner_id): Path<String>, Query(q): Query<NodesQuery>) -> Result<Json<Vec<PromptNode>>, StatusCode> {
    let kind = OwnerKind::from_db(&q.owner_kind).map_err(|_| StatusCode::BAD_REQUEST)?;
    state.storage.list_nodes(&kind, &owner_id).await.map(Json).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

pub async fn create_node(State(state): State<AppState>, Path(owner_id): Path<String>, Query(q): Query<NodesQuery>, Json(body): Json<CreateNodeBody>) -> Result<Json<PromptNode>, StatusCode> {
    let owner_kind = OwnerKind::from_db(&q.owner_kind).map_err(|_| StatusCode::BAD_REQUEST)?;
    let kind = NodeKind::from_db(&body.kind).map_err(|_| StatusCode::BAD_REQUEST)?;
    let siblings = state.storage.list_nodes(&owner_kind, &owner_id).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    enforce_two_level(&kind, &body.parent_id, &siblings)?;
    let next_order = siblings.iter().filter(|n| n.parent_id == body.parent_id).count() as i64;
    let node = match kind {
        NodeKind::Folder => PromptNode::new_folder(owner_kind, &owner_id, body.parent_id, next_order, body.tag.unwrap_or_else(|| "unnamed".into())),
        NodeKind::Ref => PromptNode::new_ref(owner_kind, &owner_id, body.parent_id, next_order, body.definition_id.ok_or(StatusCode::BAD_REQUEST)?),
        // history 是自动创建的魔法节点，不允许经此端点手动新建。
        NodeKind::History => return Err(StatusCode::BAD_REQUEST),
    };
    state.storage.create_node(&node).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(node))
}

pub async fn update_node(State(state): State<AppState>, Path(node_id): Path<String>, Json(body): Json<UpdateNodeBody>) -> Result<Json<PromptNode>, StatusCode> {
    let existing = state.storage.get_node(&node_id).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?.ok_or(StatusCode::NOT_FOUND)?;
    let owner_nodes = state.storage.list_nodes(&existing.owner_kind, &existing.owner_id).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let updated = PromptNode {
        parent_id: body.parent_id.unwrap_or(existing.parent_id),
        sort_order: body.sort_order.unwrap_or(existing.sort_order),
        tag: body.tag.unwrap_or(existing.tag),
        definition_id: body.definition_id.unwrap_or(existing.definition_id),
        enabled: body.enabled.unwrap_or(existing.enabled),
        meta: body.meta.unwrap_or(existing.meta),
        ..existing
    };
    enforce_two_level(&updated.kind, &updated.parent_id, &owner_nodes)?;
    state.storage.update_node(&updated).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(updated))
}

pub async fn delete_node(State(state): State<AppState>, Path(node_id): Path<String>) -> Result<StatusCode, StatusCode> {
    state.storage.delete_node(&node_id).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn reorder_nodes(State(state): State<AppState>, Path(owner_id): Path<String>, Query(q): Query<NodesQuery>, Json(body): Json<ReorderBody>) -> Result<StatusCode, StatusCode> {
    let kind = OwnerKind::from_db(&q.owner_kind).map_err(|_| StatusCode::BAD_REQUEST)?;
    state.storage.reorder_nodes(&kind, &owner_id, &body.ordered_ids).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(StatusCode::OK)
}
