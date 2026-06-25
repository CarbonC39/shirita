//! Asset (media) row: Uploaded images, etc., with editable, user-friendly names, for use as both avatars and backgrounds.

use serde::{Deserialize, Serialize};

/// A record in the asset database. `path` is the relative filename within the `assets` directory.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Asset {
    pub id: String,
    pub name: String,
    pub path: String,
    /// Library this asset belongs to: `"avatar"` or `"background"`.
    pub kind: String,
    /// sha256 hex of the file bytes; None until set on save / backfill.
    #[serde(default)]
    pub hash: Option<String>,
    pub created_at: String,
}

impl Asset {
    /// New asset, defaulting to the `background` library (matches the column
    /// default; avatar uploads set `kind` explicitly).
    pub fn new(name: impl Into<String>, path: impl Into<String>) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            name: name.into(),
            path: path.into(),
            kind: "background".into(),
            hash: None,
            created_at: chrono::Utc::now().to_rfc3339(),
        }
    }
}
