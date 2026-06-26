use std::path::Path as FsPath;

use axum::extract::{Multipart, Path, Query, State};
use axum::http::StatusCode;
use axum::Json;
use serde::Deserialize;
use serde_json::{json, Value};

use shirita_core::Asset;

use crate::AppState;

/// Which media library a request targets.
#[derive(Deserialize)]
pub struct KindQuery {
    pub kind: Option<String>,
}

/// Normalize a requested kind to one of the two libraries; default background.
fn norm_kind(kind: Option<&str>) -> String {
    match kind {
        Some("avatar") => "avatar".into(),
        _ => "background".into(),
    }
}

/// Parsing resource URLs on the web: relative paths → `/assets/<rel>`.
/// (The Tauri entry point returns `asset://localhost/<rel>` in M8.)
pub fn resolve_asset_url(relative: &str) -> String {
    format!("/assets/{}", relative.trim_start_matches('/'))
}

fn asset_json(a: &Asset) -> Value {
    json!({ "id": a.id, "name": a.name, "path": a.path, "kind": a.kind, "url": resolve_asset_url(&a.path) })
}

/// Delete the Asset (row + file) whose `path` matches `avatar_path`, but only
/// if nothing still references it: no `Pack.identity.avatar`, `Definition`
/// `meta.avatar`, or `Session.avatar`. Call this right after an operation
/// that may have just dropped the last reference to an avatar (a pack
/// delete, or a pack/definition avatar change) — otherwise unreferenced
/// uploads (e.g. a charcard's avatar after its pack is removed) pile up in
/// the library forever with no way to know they're unused.
pub async fn gc_avatar_if_orphaned(state: &AppState, avatar_path: &str) -> Result<(), StatusCode> {
    if avatar_path.is_empty() {
        return Ok(());
    }
    // One existence query across pack identities, definition metas and sessions,
    // instead of loading those three tables into memory and scanning.
    if state.storage.is_avatar_referenced(avatar_path).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)? {
        return Ok(());
    }
    if let Some(a) = state.storage.get_asset_by_path(avatar_path).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)? {
        let path = FsPath::new(&state.config.assets_dir).join(&a.path);
        let _ = tokio::fs::remove_file(&path).await; // best-effort; record removal is what matters
        state.storage.delete_asset(&a.id).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    }
    Ok(())
}

/// GET /api/assets[?kind=avatar|background] — list the media library, newest first.
pub async fn list(
    State(state): State<AppState>,
    Query(q): Query<KindQuery>,
) -> Result<Json<Vec<Value>>, StatusCode> {
    let assets = state
        .storage
        .list_assets(q.kind.as_deref())
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(assets.iter().map(asset_json).collect()))
}

/// POST /api/assets[?kind=avatar|background] — store the uploaded file and record
/// it with a friendly, editable name derived from the original filename.
pub async fn upload(
    State(state): State<AppState>,
    Query(q): Query<KindQuery>,
    mut multipart: Multipart,
) -> Result<Json<Value>, StatusCode> {
    // Store the first uploaded field (callers send a single `file` part).
    if let Some(field) = multipart
        .next_field()
        .await
        .map_err(|_| StatusCode::BAD_REQUEST)?
    {
        let filename = field.file_name().map(|s| s.to_string());
        let data = field.bytes().await.map_err(|_| StatusCode::BAD_REQUEST)?;
        let ext = filename
            .as_deref()
            .and_then(|f| f.rsplit('.').next())
            .filter(|e| !e.is_empty() && e.len() <= 8)
            .unwrap_or("bin");
        let stored = format!("{}.{}", uuid::Uuid::new_v4(), ext);
        let path = FsPath::new(&state.config.assets_dir).join(&stored);
        tokio::fs::write(&path, &data)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

        // Default name: the original filename without its extension, else "Image".
        let display = filename
            .as_deref()
            .map(|f| f.rsplit_once('.').map(|(stem, _)| stem).unwrap_or(f))
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .unwrap_or("Image")
            .to_string();
        let mut asset = Asset::new(display, stored);
        asset.kind = norm_kind(q.kind.as_deref());
        asset.hash = Some(shirita_core::sha256_hex(data.as_ref()));
        if state.storage.create_asset(&asset).await.is_err() {
            // The DB row is what makes the file discoverable; if it didn't land,
            // drop the file we just wrote rather than leaking an orphan upload.
            let _ = tokio::fs::remove_file(&path).await;
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }
        return Ok(Json(asset_json(&asset)));
    }
    Err(StatusCode::BAD_REQUEST)
}

#[derive(Deserialize)]
pub struct RenameAsset {
    pub name: String,
}

/// PUT /api/assets/{id} — rename a library entry.
pub async fn rename(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<RenameAsset>,
) -> Result<StatusCode, StatusCode> {
    state.storage.rename_asset(&id, body.name.trim()).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(StatusCode::OK)
}

/// DELETE /api/assets/{id} — remove the record and its file.
pub async fn delete(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<StatusCode, StatusCode> {
    if let Some(asset) = state.storage.get_asset(&id).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)? {
        let path = FsPath::new(&state.config.assets_dir).join(&asset.path);
        let _ = tokio::fs::remove_file(&path).await; // best-effort; record removal is what matters
        state.storage.delete_asset(&id).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    }
    Ok(StatusCode::NO_CONTENT)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_prefixes_assets() {
        assert_eq!(resolve_asset_url("a.png"), "/assets/a.png");
        assert_eq!(resolve_asset_url("/a.png"), "/assets/a.png");
    }
}
