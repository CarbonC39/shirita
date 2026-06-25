//! Parses the asset ID attached to a message into an image data URL that the provider can consume directly.
//! Asset metadata (ID → relative path) comes from `Storage::get_asset`; the bytes themselves are
//! read from `config.assets_dir` using the relative path (matching the disk layout of `routes::assets`).

use base64::Engine;

use crate::storage::Storage;

/// Guess the MIME type based on the file name/path extension; use `application/octet-stream` as a fallback for unknown extensions.
/// (The provider will throw an error for unsupported types, which is safer than silently discarding the image.)
pub fn mime_from_ext(path: &str) -> &'static str {
    let ext = path.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
    match ext.as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        _ => "application/octet-stream",
    }
}

/// Parse a list of URLs with asset IDs in the format `data:<mime>;base64,<data>`, preserving the input order.
/// Assets that cannot be found or files that cannot be read are skipped (without blocking the request; these are typically deleted assets).
pub async fn resolve_images(storage: &dyn Storage, assets_dir: &str, attachment_ids: &[String]) -> Vec<String> {
    let mut out = Vec::new();
    for id in attachment_ids {
        let Ok(Some(asset)) = storage.get_asset(id).await else { continue };
        let full = format!("{}/{}", assets_dir.trim_end_matches('/'), asset.path.trim_start_matches('/'));
        let Ok(bytes) = tokio::fs::read(&full).await else { continue };
        let mime = mime_from_ext(&asset.path);
        let data = base64::engine::general_purpose::STANDARD.encode(&bytes);
        out.push(format!("data:{mime};base64,{data}"));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::asset::Asset;
    use crate::storage::sqlite::SqliteStorage;

    #[test]
    fn mime_from_ext_known_extensions() {
        assert_eq!(mime_from_ext("a.png"), "image/png");
        assert_eq!(mime_from_ext("a.JPG"), "image/jpeg");
        assert_eq!(mime_from_ext("a.jpeg"), "image/jpeg");
        assert_eq!(mime_from_ext("a.gif"), "image/gif");
        assert_eq!(mime_from_ext("a.webp"), "image/webp");
        assert_eq!(mime_from_ext("a.bin"), "application/octet-stream");
        assert_eq!(mime_from_ext("noext"), "application/octet-stream");
    }

    async fn temp_storage_and_dir() -> (SqliteStorage, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("test.db");
        let storage = SqliteStorage::connect(db.to_str().unwrap()).await.unwrap();
        storage.run_migrations().await.unwrap();
        (storage, dir)
    }

    #[tokio::test]
    async fn resolves_known_assets_to_data_urls_in_order() {
        let (storage, dir) = temp_storage_and_dir().await;
        let assets_dir = dir.path().join("assets");
        tokio::fs::create_dir_all(&assets_dir).await.unwrap();
        tokio::fs::write(assets_dir.join("pic.png"), b"\x89PNG-fake-bytes").await.unwrap();

        let asset = Asset { id: "a1".into(), name: "pic".into(), path: "pic.png".into(), kind: "background".into(), hash: None, created_at: "".into() };
        storage.create_asset(&asset).await.unwrap();

        let urls = resolve_images(&storage, assets_dir.to_str().unwrap(), &["a1".to_string()]).await;
        assert_eq!(urls.len(), 1);
        assert!(urls[0].starts_with("data:image/png;base64,"));
    }

    #[tokio::test]
    async fn skips_unknown_asset_ids() {
        let (storage, dir) = temp_storage_and_dir().await;
        let urls = resolve_images(&storage, dir.path().to_str().unwrap(), &["missing".to_string()]).await;
        assert!(urls.is_empty());
    }

    #[tokio::test]
    async fn skips_assets_whose_file_is_missing_on_disk() {
        let (storage, dir) = temp_storage_and_dir().await;
        let asset = Asset { id: "a1".into(), name: "pic".into(), path: "gone.png".into(), kind: "background".into(), hash: None, created_at: "".into() };
        storage.create_asset(&asset).await.unwrap();
        let urls = resolve_images(&storage, dir.path().to_str().unwrap(), &["a1".to_string()]).await;
        assert!(urls.is_empty());
    }
}
