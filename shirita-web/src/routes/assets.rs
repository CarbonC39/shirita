use std::path::Path as FsPath;

use axum::extract::{Multipart, State};
use axum::http::StatusCode;
use axum::Json;
use serde_json::{json, Value};

use crate::AppState;

/// Web 下的资源 URL 解析：相对路径 → `/assets/<rel>`。
/// （Tauri 入口在 M8 返回 `asset://localhost/<rel>`。）
pub fn resolve_asset_url(relative: &str) -> String {
    format!("/assets/{}", relative.trim_start_matches('/'))
}

pub async fn upload(
    State(state): State<AppState>,
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
        let name = format!("{}.{}", uuid::Uuid::new_v4(), ext);
        let path = FsPath::new(&state.config.assets_dir).join(&name);
        tokio::fs::write(&path, &data)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        return Ok(Json(json!({ "path": name, "url": resolve_asset_url(&name) })));
    }
    Err(StatusCode::BAD_REQUEST)
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
