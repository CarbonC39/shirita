//! 资源库（媒体）一行：上传后的图片等，带可改的友好名字，供头像/背景共用。

use serde::{Deserialize, Serialize};

/// 资源库中的一条记录。`path` 为 assets 目录下的相对文件名。
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
