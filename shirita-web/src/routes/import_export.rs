use std::collections::HashMap;

use axum::extract::{Multipart, Query, State};
use axum::http::StatusCode;
use axum::Json;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use shirita_core::{Definition, NodeKind, OwnerKind, PromptNode, Template};

use crate::AppState;

/// 同名冲突的全局策略。
#[derive(Debug, Clone, Copy)]
pub enum OnConflict {
    Skip,
    Overwrite,
    Duplicate,
}

impl OnConflict {
    fn parse(s: Option<&str>) -> Self {
        match s {
            Some("overwrite") => OnConflict::Overwrite,
            Some("duplicate") => OnConflict::Duplicate,
            _ => OnConflict::Skip, // 默认 + 未知
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct ImportQuery {
    pub on_conflict: Option<String>,
}

#[derive(Debug, Default, Serialize)]
pub struct ImportSummary {
    pub created: Vec<ImportItem>,
    pub skipped: Vec<ImportItem>,
    pub overwritten: Vec<ImportItem>,
}

#[derive(Debug, Serialize)]
pub struct ImportItem {
    pub kind: String,
    pub id: String,
    pub name: String,
}

fn item(kind: &str, id: &str, name: &str) -> ImportItem {
    ImportItem { kind: kind.into(), id: id.into(), name: name.into() }
}

/// 按 name+def_type 判重，依 `on_conflict` 落库定义；累加进 summary。
async fn persist_defs(
    state: &AppState,
    defs: Vec<Definition>,
    oc: OnConflict,
    summary: &mut ImportSummary,
) -> Result<(), StatusCode> {
    let existing = state.storage.list_definitions().await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    for mut d in defs {
        let dup = existing.iter().find(|e| e.name == d.name && e.def_type == d.def_type).cloned();
        match (dup, oc) {
            (Some(ex), OnConflict::Skip) => summary.skipped.push(item("definition", &ex.id, &ex.name)),
            (Some(ex), OnConflict::Overwrite) => {
                // 原地更新：保留 ex.id，绝不删除（护 ON DELETE SET NULL 引用）。
                d.id = ex.id.clone();
                state.storage.update_definition(&d).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
                summary.overwritten.push(item("definition", &d.id, &d.name));
            }
            (_, OnConflict::Duplicate) | (None, _) => {
                state.storage.create_definition(&d).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
                summary.created.push(item("definition", &d.id, &d.name));
            }
        }
    }
    Ok(())
}

/// 把整张 PNG 存进 assets 目录并登记 Asset，返回存储文件名（写入定义 meta.avatar）。
async fn save_png_asset(state: &AppState, bytes: &[u8], display: &str) -> Result<String, StatusCode> {
    use std::path::Path as FsPath;
    let stored = format!("{}.png", uuid::Uuid::new_v4());
    let path = FsPath::new(&state.config.assets_dir).join(&stored);
    tokio::fs::create_dir_all(&state.config.assets_dir).await.ok();
    tokio::fs::write(&path, bytes).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let asset = shirita_core::Asset::new(display, stored.clone());
    state.storage.create_asset(&asset).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(stored)
}

/// 读取首个 multipart 字段的字节。
async fn first_field_bytes(mut mp: Multipart) -> Result<Vec<u8>, StatusCode> {
    let field = mp.next_field().await.map_err(|_| StatusCode::BAD_REQUEST)?.ok_or(StatusCode::BAD_REQUEST)?;
    let bytes = field.bytes().await.map_err(|_| StatusCode::BAD_REQUEST)?;
    Ok(bytes.to_vec())
}

const PNG_SIG: [u8; 8] = [0x89, b'P', b'N', b'G', b'\r', b'\n', 0x1a, b'\n'];

/// 把一张 ST 角色卡 JSON（可带头像文件名）转成定义列表（char + 内嵌世界书）。
fn card_to_defs(card: &Value, avatar: Option<&str>) -> Vec<Definition> {
    let (mut ch, book) = shirita_core::charcard_to_defs(card);
    if let (Some(av), Some(obj)) = (avatar, ch.meta.as_object_mut()) {
        obj.insert("avatar".into(), json!(av));
    }
    let mut all = vec![ch];
    all.extend(book);
    all
}

/// POST /api/import — multipart 单 `file`。按内容 sniff 来源并落库。
pub async fn import(
    State(state): State<AppState>,
    Query(q): Query<ImportQuery>,
    mp: Multipart,
) -> Result<Json<ImportSummary>, StatusCode> {
    let oc = OnConflict::parse(q.on_conflict.as_deref());
    let bytes = first_field_bytes(mp).await?;
    let mut summary = ImportSummary::default();

    // 1) PNG → ST 角色卡 + 头像。
    if bytes.len() >= 8 && bytes[..8] == PNG_SIG {
        let card = shirita_core::read_card_json(&bytes).map_err(|_| StatusCode::BAD_REQUEST)?;
        let name = card.get("data").and_then(|d| d.get("name")).and_then(|v| v.as_str()).unwrap_or("character");
        let avatar = save_png_asset(&state, &bytes, name).await?;
        persist_defs(&state, card_to_defs(&card, Some(&avatar)), oc, &mut summary).await?;
        return Ok(Json(summary));
    }

    // 2) 否则按 JSON sniff。
    let v: Value = serde_json::from_slice(&bytes).map_err(|_| StatusCode::BAD_REQUEST)?;
    match v.get("format").and_then(|f| f.as_str()) {
        Some("shirita.definition") => {
            match shirita_core::parse_portable(&v).map_err(|_| StatusCode::BAD_REQUEST)? {
                shirita_core::PortableDoc::Definition(d) => persist_defs(&state, vec![d], oc, &mut summary).await?,
                _ => return Err(StatusCode::BAD_REQUEST),
            }
        }
        Some("shirita.template") => import_template_bundle(&state, &v, oc, &mut summary).await?,
        _ => {
            let is_card = v.get("spec").and_then(|s| s.as_str()).map(|s| s.contains("chara_card")).unwrap_or(false)
                || v.get("data").and_then(|d| d.get("name")).is_some()
                || (v.get("name").is_some() && v.get("description").is_some());
            if is_card {
                persist_defs(&state, card_to_defs(&v, None), oc, &mut summary).await?;
            } else if v.get("entries").is_some() {
                persist_defs(&state, shirita_core::worldinfo_to_defs(&v), oc, &mut summary).await?;
            } else {
                return Err(StatusCode::BAD_REQUEST);
            }
        }
    }
    Ok(Json(summary))
}

/// 兼容薄包装：固定 ST 角色卡 JSON 来源，转调统一落库逻辑（默认 skip）。
pub async fn import_charcard(
    State(state): State<AppState>,
    Json(body): Json<Value>,
) -> Result<Json<ImportSummary>, StatusCode> {
    let mut summary = ImportSummary::default();
    persist_defs(&state, card_to_defs(&body, None), OnConflict::Skip, &mut summary).await?;
    Ok(Json(summary))
}

/// 兼容薄包装：固定 ST 世界书 JSON 来源。
pub async fn import_worldinfo(
    State(state): State<AppState>,
    Json(body): Json<Value>,
) -> Result<Json<ImportSummary>, StatusCode> {
    let mut summary = ImportSummary::default();
    persist_defs(&state, shirita_core::worldinfo_to_defs(&body), OnConflict::Skip, &mut summary).await?;
    Ok(Json(summary))
}

/// 还原 shirita.template bundle：bundle 为原子单位，按模板名决策。
/// skip（存在且 Skip）→ 整 bundle 跳过；否则全新建（模板+定义+节点，local_id 重映射为新 UUID）。
/// **绝不删除现有模板**（护 M4 惰性 Fork：未 materialize 会话直接引用模板节点）。
async fn import_template_bundle(
    state: &AppState,
    v: &Value,
    oc: OnConflict,
    summary: &mut ImportSummary,
) -> Result<(), StatusCode> {
    let doc = shirita_core::parse_portable(v).map_err(|_| StatusCode::BAD_REQUEST)?;
    let (name, meta, nodes, defs) = match doc {
        shirita_core::PortableDoc::Template { name, meta, nodes, defs } => (name, meta, nodes, defs),
        _ => return Err(StatusCode::BAD_REQUEST),
    };

    // 模板冲突：仅 Skip 时跳过同名；overwrite 对模板等同 duplicate（绝不删旧模板）。
    if matches!(oc, OnConflict::Skip) {
        let templates = state.storage.list_templates().await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        if let Some(ex) = templates.iter().find(|t| t.name == name) {
            summary.skipped.push(item("template", &ex.id, &ex.name));
            return Ok(());
        }
    }

    // 1) 新建模板。
    let mut tmpl = Template::new(&name);
    tmpl.meta = meta;
    state.storage.create_template(&tmpl).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // 2) 新建定义，建 local_id -> 新定义 id 映射（bundle 内定义随模板原子新建，不按 name+type 去重）。
    let mut def_map: HashMap<String, String> = HashMap::new();
    for pd in &defs {
        let mut d = Definition::new(&pd.def_type, &pd.name, &pd.content);
        d.meta = pd.meta.clone();
        state.storage.create_definition(&d).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        def_map.insert(pd.local_id.clone(), d.id.clone());
    }

    // 3) 预分配节点新 UUID（供 parent 重指）。
    let node_map: HashMap<String, String> =
        nodes.iter().map(|n| (n.local_id.clone(), uuid::Uuid::new_v4().to_string())).collect();

    // 拓扑插入：父必须先于子（parent_id REFERENCES prompt_nodes(id)）。bundle 节点顺序不保证父在前
    // （导出侧 list_nodes 在 sort_order 相等时排序不定），故按"父已插入"分层插入。
    let mut inserted: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut remaining: Vec<&shirita_core::PortableNode> = nodes.iter().collect();
    loop {
        let mut progressed = false;
        let mut still: Vec<&shirita_core::PortableNode> = Vec::new();
        for pn in remaining {
            // 父在 bundle 内但尚未插入 → 留待下一轮；父不在 bundle 内则视为根。
            let parent_pending = match &pn.parent_local_id {
                Some(p) => node_map.contains_key(p) && !inserted.contains(p),
                None => false,
            };
            if parent_pending {
                still.push(pn);
                continue;
            }
            // ref 的 definition_id 经 def_map 重指；缺失则跳过该节点 + warn。
            let definition_id = match (&pn.kind, &pn.def_local_id) {
                (NodeKind::Ref, Some(dl)) => match def_map.get(dl) {
                    Some(real) => Some(real.clone()),
                    None => {
                        tracing::warn!(local_id = %pn.local_id, "template import: ref def_local_id missing, skipping node");
                        inserted.insert(pn.local_id.clone());
                        progressed = true;
                        continue;
                    }
                },
                _ => None,
            };
            let node = PromptNode {
                id: node_map[&pn.local_id].clone(),
                owner_kind: OwnerKind::Template,
                owner_id: tmpl.id.clone(),
                parent_id: pn.parent_local_id.as_ref().and_then(|p| node_map.get(p)).cloned(),
                sort_order: pn.sort_order,
                kind: pn.kind.clone(),
                tag: pn.tag.clone(),
                definition_id,
                enabled: pn.enabled,
                created_at: chrono::Utc::now().to_rfc3339(),
            };
            state.storage.create_node(&node).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            inserted.insert(pn.local_id.clone());
            progressed = true;
        }
        remaining = still;
        if remaining.is_empty() || !progressed {
            break; // 全部插入完，或剩余为循环引用（兜底防死循环）。
        }
    }

    summary.created.push(item("template", &tmpl.id, &tmpl.name));
    Ok(())
}
