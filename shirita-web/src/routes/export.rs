use std::collections::HashMap;
use std::io::{Cursor, Write};
use std::path::Path as FsPath;

use axum::extract::{Path, State};
use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;

use shirita_core::{Definition, OwnerKind};

use crate::AppState;

/// File name sanitization: Retain only alphanumeric characters, `/-`, and `/_`; convert all others to `/_`.
fn safe_filename(name: &str) -> String {
    let s: String = name
        .chars()
        .map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '_' })
        .collect();
    if s.is_empty() { "export".into() } else { s }
}

/// GET /api/definitions/{id}/export — Returns the original JSON for a single definition (with a download header).
pub async fn export_definition(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, StatusCode> {
    let def = state
        .storage
        .get_definition(&id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;
    let v = shirita_core::export_definition(&def);
    let cd = format!("attachment; filename=\"{}.json\"", safe_filename(&def.name));
    Ok(([(header::CONTENT_DISPOSITION, cd)], Json(v)))
}

/// GET /api/templates/{id}/export — Original JSON for the “Enabled” section of the template (includes download header).
pub async fn export_template(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, StatusCode> {
    let tmpl = state
        .storage
        .get_template(&id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;
    let nodes = state
        .storage
        .list_nodes(&OwnerKind::Template, &id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let all = state.storage.list_definitions().await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let defs: HashMap<String, Definition> = all.into_iter().map(|d| (d.id.clone(), d)).collect();
    let v = shirita_core::export_template(&tmpl, &nodes, &defs);
    let cd = format!("attachment; filename=\"{}.json\"", safe_filename(&tmpl.name));
    Ok(([(header::CONTENT_DISPOSITION, cd)], Json(v)))
}

/// GET /api/packs/{id}/export — a zip bundle (manifest.json + assets/<file>)
/// when the pack references binary, else a plain `shirita.pack` JSON. The
/// download filename (.zip / .json) is set in Content-Disposition.
pub async fn export_pack(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Response, StatusCode> {
    let pack = state
        .storage
        .get_pack(&id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;
    let nodes = state
        .storage
        .list_nodes(&OwnerKind::Pack, &id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let all = state.storage.list_definitions().await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let defs: HashMap<String, Definition> = all.into_iter().map(|d| (d.id.clone(), d)).collect();
    let manifest = shirita_core::export_pack(&pack, &nodes, &defs);
    let base = safe_filename(&pack.name);

    // Read every referenced asset that exists on disk; missing ones are skipped
    // (their manifest ref is blanked at import by rewrite_pack_assets).
    let mut assets: Vec<(String, Vec<u8>)> = Vec::new();
    for rel in shirita_core::collect_pack_assets(&manifest) {
        let path = FsPath::new(&state.config.assets_dir).join(&rel);
        match tokio::fs::read(&path).await {
            Ok(bytes) => assets.push((rel, bytes)),
            Err(_) => tracing::warn!(rel = %rel, "export_pack: asset file missing, skipping"),
        }
    }

    // No binary → plain JSON.
    if assets.is_empty() {
        let cd = format!("attachment; filename=\"{base}.json\"");
        return Ok(([(header::CONTENT_DISPOSITION, cd)], Json(manifest)).into_response());
    }

    // Build the zip in memory: manifest.json + assets/<rel>.
    let mut cursor = Cursor::new(Vec::<u8>::new());
    {
        let mut zw = zip::ZipWriter::new(&mut cursor);
        let opts = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated);
        let manifest_bytes =
            serde_json::to_vec(&manifest).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        zw.start_file("manifest.json", opts).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        zw.write_all(&manifest_bytes).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        for (rel, bytes) in &assets {
            zw.start_file(format!("assets/{rel}"), opts).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            zw.write_all(bytes).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        }
        zw.finish().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    }
    let bytes = cursor.into_inner();
    let cd = format!("attachment; filename=\"{base}.zip\"");
    Ok((
        [
            (header::CONTENT_TYPE, "application/zip".to_string()),
            (header::CONTENT_DISPOSITION, cd),
        ],
        bytes,
    )
        .into_response())
}
