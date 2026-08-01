use std::collections::HashMap;
use std::io::Read;

use axum::extract::{Multipart, Query, State};
use axum::http::StatusCode;
use axum::Json;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use shirita_core::{
    collect_pack_assets, parse_portable, rewrite_pack_assets,
    Asset, Definition, NodeKind, OwnerKind, Pack, PortableDoc, PromptNode,
    Template,
};

use crate::AppState;

/// Global strategy for name conflicts.
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
            _ => OnConflict::Skip, // default + unknown
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

const MAX_ZIP_ENTRIES: usize = 512;
const MAX_ENTRY_BYTES: u64 = 32 * 1024 * 1024; // 32 MiB per file
const MAX_TOTAL_BYTES: u64 = 64 * 1024 * 1024; // 64 MiB total decompressed

/// Safely unpack a `shirita.pack` zip into (manifest, `assets/<rel>` → bytes).
/// Rejects unsafe paths (`..`/absolute via `enclosed_name`), nested `assets/`
/// entries, and over-cap entry counts / per-entry / total decompressed sizes.
fn unzip_pack(bytes: &[u8]) -> Result<(Value, HashMap<String, Vec<u8>>), StatusCode> {
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes)).map_err(|_| StatusCode::BAD_REQUEST)?;
    if zip.len() > MAX_ZIP_ENTRIES {
        return Err(StatusCode::BAD_REQUEST);
    }
    let mut manifest: Option<Value> = None;
    let mut assets: HashMap<String, Vec<u8>> = HashMap::new();
    let mut total: u64 = 0;
    for i in 0..zip.len() {
        let entry = zip.by_index(i).map_err(|_| StatusCode::BAD_REQUEST)?;
        // `enclosed_name` returns None for traversal/absolute paths.
        let name = match entry.enclosed_name() {
            Some(p) => p.to_string_lossy().replace('\\', "/"),
            None => return Err(StatusCode::BAD_REQUEST),
        };
        let is_dir = entry.is_dir();
        let declared = entry.size();
        if is_dir {
            continue;
        }
        if declared > MAX_ENTRY_BYTES {
            return Err(StatusCode::BAD_REQUEST);
        }
        // Read with a hard cap — the declared size can lie.
        let mut buf = Vec::new();
        entry.take(MAX_ENTRY_BYTES + 1).read_to_end(&mut buf).map_err(|_| StatusCode::BAD_REQUEST)?;
        if buf.len() as u64 > MAX_ENTRY_BYTES {
            return Err(StatusCode::BAD_REQUEST);
        }
        total += buf.len() as u64;
        if total > MAX_TOTAL_BYTES {
            return Err(StatusCode::BAD_REQUEST);
        }
        if name == "manifest.json" {
            manifest = Some(serde_json::from_slice(&buf).map_err(|_| StatusCode::BAD_REQUEST)?);
        } else if let Some(rel) = name.strip_prefix("assets/") {
            // Flat names only — no nested directories under assets/.
            if rel.is_empty() || rel.contains('/') {
                return Err(StatusCode::BAD_REQUEST);
            }
            assets.insert(rel.to_string(), buf);
        }
        // Any other top-level entry is ignored.
    }
    let manifest = manifest.ok_or(StatusCode::BAD_REQUEST)?;
    Ok((manifest, assets))
}

/// Restore a `shirita.pack` manifest + its bundled asset bytes: hash-dedup each
/// referenced asset (reuse an existing/just-queued row by content hash, else
/// write the file + register a new Asset), rewrite the manifest's designated
/// asset fields to the stored names (blanking refs absent from the bundle), then
/// atomically create the pack, its definitions and its nodes.
async fn persist_pack_bundle(
    state: &AppState,
    manifest: &Value,
    zip_assets: &HashMap<String, Vec<u8>>,
    oc: OnConflict,
    summary: &mut ImportSummary,
) -> Result<(), StatusCode> {
    use std::path::Path as FsPath;

    // Skip an existing same-name pack (peek before the full parse/restore).
    let name = manifest.get("pack").and_then(|p| p.get("name")).and_then(|v| v.as_str()).unwrap_or("Pack").to_string();
    if matches!(oc, OnConflict::Skip) {
        if let Some(ex) = state.storage.get_pack_by_name(&name).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)? {
            summary.skipped.push(item("pack", &ex.id, &ex.name));
            return Ok(());
        }
    }

    // 1) Hash-dedup restore. Only assets BOTH designated AND present in the zip
    //    are restored; the old→new map drives the rewrite (missing → blanked).
    tokio::fs::create_dir_all(&state.config.assets_dir).await.ok();
    let mut rename: HashMap<String, String> = HashMap::new();
    let mut new_assets: Vec<Asset> = Vec::new();
    let mut by_hash: HashMap<String, String> = HashMap::new(); // in-batch dedup
    for rel in collect_pack_assets(manifest) {
        let Some(bytes) = zip_assets.get(&rel) else { continue };
        let hash = shirita_core::sha256_hex(bytes);
        let stored = if let Some(p) = by_hash.get(&hash) {
            p.clone()
        } else if let Some(ex) =
            state.storage.find_asset_by_hash(&hash).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        {
            ex.path
        } else {
            let ext = FsPath::new(&rel).extension().and_then(|e| e.to_str()).unwrap_or("bin");
            let stored = format!("{}.{}", uuid::Uuid::new_v4(), ext);
            tokio::fs::write(FsPath::new(&state.config.assets_dir).join(&stored), bytes)
                .await
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            let mut a = Asset::new(&rel, stored.clone());
            a.kind = "avatar".into();
            a.hash = Some(hash.clone());
            new_assets.push(a);
            stored
        };
        by_hash.insert(hash, stored.clone());
        rename.insert(rel, stored);
    }

    // 2) Rewrite designated refs to stored names (unmapped → blanked).
    let rewritten = rewrite_pack_assets(manifest, &rename);

    // 3) Parse to a portable pack; build a fresh pack (new UUID) + entities.
    let (pname, identity, meta, pnodes, pdefs) =
        match parse_portable(&rewritten).map_err(|_| StatusCode::BAD_REQUEST)? {
            PortableDoc::Pack { name, identity, meta, nodes, defs } => (name, identity, meta, nodes, defs),
            _ => return Err(StatusCode::BAD_REQUEST),
        };
    let mut pack = Pack::new(&pname);
    pack.identity = identity;
    pack.meta = meta;

    // Definitions: local_id → new id (bundle defs created fresh, like template import).
    let mut def_map: HashMap<String, String> = HashMap::new();
    let mut out_defs: Vec<Definition> = Vec::new();
    for pd in &pdefs {
        let mut d = Definition::new(&pd.def_type, &pd.name, &pd.content);
        d.meta = pd.meta.clone();
        def_map.insert(pd.local_id.clone(), d.id.clone());
        out_defs.push(d);
    }

    // Nodes: pre-allocate new ids, then emit in parent-before-child order
    // (mirrors import_template_bundle's topological layering).
    let node_map: HashMap<String, String> =
        pnodes.iter().map(|n| (n.local_id.clone(), uuid::Uuid::new_v4().to_string())).collect();
    let mut out_nodes: Vec<PromptNode> = Vec::new();
    let mut inserted: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut remaining: Vec<&shirita_core::PortableNode> = pnodes.iter().collect();
    loop {
        let mut progressed = false;
        let mut still: Vec<&shirita_core::PortableNode> = Vec::new();
        for pn in remaining {
            let parent_pending = match &pn.parent_local_id {
                Some(p) => node_map.contains_key(p) && !inserted.contains(p),
                None => false,
            };
            if parent_pending {
                still.push(pn);
                continue;
            }
            let definition_id = match (&pn.kind, &pn.def_local_id) {
                (NodeKind::Ref, Some(dl)) => match def_map.get(dl) {
                    Some(real) => Some(real.clone()),
                    None => {
                        tracing::warn!(local_id = %pn.local_id, "pack import: ref def_local_id missing, skipping node");
                        inserted.insert(pn.local_id.clone());
                        progressed = true;
                        continue;
                    }
                },
                _ => None,
            };
            out_nodes.push(PromptNode {
                id: node_map[&pn.local_id].clone(),
                owner_kind: OwnerKind::Pack,
                owner_id: pack.id.clone(),
                parent_id: pn.parent_local_id.as_ref().and_then(|p| node_map.get(p)).cloned(),
                sort_order: pn.sort_order,
                kind: pn.kind.clone(),
                tag: pn.tag.clone(),
                definition_id,
                enabled: pn.enabled,
                created_at: chrono::Utc::now().to_rfc3339(),
                meta: pn.meta.clone(),
            });
            inserted.insert(pn.local_id.clone());
            progressed = true;
        }
        remaining = still;
        if remaining.is_empty() || !progressed {
            break;
        }
    }

    // 4) One transaction: assets + pack + defs + nodes (full rollback on any error).
    state
        .storage
        .import_pack(&pack, &out_defs, &out_nodes, &new_assets)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    summary.created.push(item("pack", &pack.id, &pack.name));
    Ok(())
}

/// Check for duplicates based on name+def_type; define storage based on `on_conflict`; add to the summary.
async fn persist_defs(
    state: &AppState,
    defs: Vec<Definition>,
    oc: OnConflict,
    summary: &mut ImportSummary,
) -> Result<(), StatusCode> {
    let existing = state.storage.list_definitions().await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    for mut d in defs {
        // Skip empty content-bearing defs (cleanliness), but never drop identity
        // anchors (char/persona with a name or avatar) or meta-only types whose
        // payload lives in meta (regex_rule/first_message).
        let meta_only = matches!(d.def_type.as_str(), "regex_rule" | "first_message");
        let is_anchor = matches!(d.def_type.as_str(), "char" | "persona")
            && (!d.name.trim().is_empty()
                || d.meta.get("avatar").and_then(|v| v.as_str()).map(|s| !s.is_empty()).unwrap_or(false));
        if d.content.trim().is_empty() && !meta_only && !is_anchor {
            continue;
        }
        let dup = existing.iter().find(|e| e.name == d.name && e.def_type == d.def_type).cloned();
        match (dup, oc) {
            (Some(ex), OnConflict::Skip) => summary.skipped.push(item("definition", &ex.id, &ex.name)),
            (Some(ex), OnConflict::Overwrite) => {
                // Update in place: Preserve ex.id; never delete it (to preserve the ON DELETE SET NULL reference).
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

/// Read the first multipart field's bytes.
async fn first_field(mut mp: Multipart) -> Result<Vec<u8>, StatusCode> {
    let field = mp.next_field().await.map_err(|_| StatusCode::BAD_REQUEST)?.ok_or(StatusCode::BAD_REQUEST)?;
    let bytes = field.bytes().await.map_err(|_| StatusCode::BAD_REQUEST)?;
    Ok(bytes.to_vec())
}

/// POST /api/import — multipart request containing a single `file`. Accepts
/// only the three explicit Shirita-native portable formats:
///  1. a `shirita.pack` ZIP bundle (manifest.json + assets/), detected by ZIP
///     signature and validated by requiring a `shirita.pack` manifest;
///  2. otherwise the payload must parse as JSON and carry an explicit
///     `format` of `shirita.definition`, `shirita.template`, or `shirita.pack`;
///  3. everything else (ST card/preset/World Info JSON, arbitrary PNG bytes,
///     unknown JSON) is rejected with `400 Bad Request`.
pub async fn import(
    State(state): State<AppState>,
    Query(q): Query<ImportQuery>,
    mp: Multipart,
) -> Result<Json<ImportSummary>, StatusCode> {
    let oc = OnConflict::parse(q.on_conflict.as_deref());
    let bytes = first_field(mp).await?;
    let mut summary = ImportSummary::default();

    // 1) ZIP signature → shirita.pack bundle (manifest.json + assets/<file>).
    if bytes.len() >= 4 && bytes[..4] == [0x50, 0x4B, 0x03, 0x04] {
        let (manifest, zip_assets) = unzip_pack(&bytes)?;
        persist_pack_bundle(&state, &manifest, &zip_assets, oc, &mut summary).await?;
        return Ok(Json(summary));
    }

    // 2) Otherwise it must be JSON with an explicit native `format` value.
    let v: Value = serde_json::from_slice(&bytes).map_err(|_| StatusCode::BAD_REQUEST)?;
    match v.get("format").and_then(|f| f.as_str()) {
        Some("shirita.definition") => {
            match shirita_core::parse_portable(&v).map_err(|_| StatusCode::BAD_REQUEST)? {
                shirita_core::PortableDoc::Definition(d) => persist_defs(&state, vec![d], oc, &mut summary).await?,
                _ => return Err(StatusCode::BAD_REQUEST),
            }
        }
        Some("shirita.template") => import_template_bundle(&state, &v, oc, &mut summary).await?,
        Some("shirita.pack") => {
            persist_pack_bundle(&state, &v, &HashMap::new(), oc, &mut summary).await?;
        }
        // No recognized native discriminator → reject. Unknown formats are not
        // guessed by structure; accepting them would keep compatibility logic
        // in the main application.
        _ => return Err(StatusCode::BAD_REQUEST),
    }
    Ok(Json(summary))
}

/// Restore the shirita.template bundle: The bundle is an atomic unit, and decisions are made based on the template name.
/// skip (if present and set to Skip) → Skip the entire bundle; otherwise, create a new one (template + definitions + nodes, with local_id remapped to a new UUID).
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

    // Template conflict: When using “Skip,” templates with the same name are skipped; ‘overwrite’ is equivalent to “duplicate” for templates (the old template is never deleted).
    if matches!(oc, OnConflict::Skip) {
        if let Some(ex) = state.storage.get_template_by_name(&name).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)? {
            summary.skipped.push(item("template", &ex.id, &ex.name));
            return Ok(());
        }
    }

    // 1) New template (stored atomically in the database within a single `import_template` transaction, along with the definitions and nodes below).
    let mut tmpl = Template::new(&name);
    tmpl.meta = meta;

    // 2) Create a new definition: create a mapping from `local_id` to the newly defined `id` (definitions within the bundle are created atomically based on the template; duplicate entries are not filtered based on `name` and `type`).
    let mut def_map: HashMap<String, String> = HashMap::new();
    let mut out_defs: Vec<Definition> = Vec::new();
    for pd in &defs {
        let mut d = Definition::new(&pd.def_type, &pd.name, &pd.content);
        d.meta = pd.meta.clone();
        def_map.insert(pd.local_id.clone(), d.id.clone());
        out_defs.push(d);
    }

    // 3) Pre-allocate a new UUID for the node (for the parent to refer to).
    let node_map: HashMap<String, String> =
        nodes.iter().map(|n| (n.local_id.clone(), uuid::Uuid::new_v4().to_string())).collect();

    // Topological insertion: Parents must be inserted before children (parent_id REFERENCES prompt_nodes(id)). The order of bundle nodes is not guaranteed to place parents first
    // (on the export side, list_nodes are not sorted in a specific order when sort_order is equal), so insertion is performed in layers based on “parents already inserted.”
    let mut inserted: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut out_nodes: Vec<PromptNode> = Vec::new();
    let mut remaining: Vec<&shirita_core::PortableNode> = nodes.iter().collect();
    loop {
        let mut progressed = false;
        let mut still: Vec<&shirita_core::PortableNode> = Vec::new();
        for pn in remaining {
            // If the parent is in the bundle but has not yet been inserted → defer to the next round; if the parent is not in the bundle, treat it as the root.
            let parent_pending = match &pn.parent_local_id {
                Some(p) => node_map.contains_key(p) && !inserted.contains(p),
                None => false,
            };
            if parent_pending {
                still.push(pn);
                continue;
            }
            // The `definition_id` of `ref` is re-mapped by `def_map`; if it is missing, skip the node and issue a warning.
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
            out_nodes.push(PromptNode {
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
                meta: pn.meta.clone(),
            });
            inserted.insert(pn.local_id.clone());
            progressed = true;
        }
        remaining = still;
        if remaining.is_empty() || !progressed {
            break; // All items have been inserted, or the remaining items are circular references (fallback to prevent an infinite loop).
        }
    }

    // Templates + definitions + nodes (sorted with parent nodes first) are committed to the database as a single atomic operation: if any step fails, the entire transaction is rolled back,
    // leaving no orphaned template or definition lines behind.
    state
        .storage
        .import_template(&tmpl, &out_defs, &out_nodes)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    summary.created.push(item("template", &tmpl.id, &tmpl.name));
    Ok(())
}
