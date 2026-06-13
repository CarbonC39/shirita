//! 运行时配置：DATABASE_PATH / ASSETS_DIR / TOKEN_SECRET。

use crate::{Error, Result};

pub struct Config {
    pub database_path: String,
    pub assets_dir: String,
    pub token_secret: String,
    pub openai_base_url: String,
    pub openai_api_key: String,
    pub openai_model: String,
}

impl Config {
    pub fn new(
        database_path: impl Into<String>,
        assets_dir: impl Into<String>,
        token_secret: impl Into<String>,
    ) -> Result<Self> {
        let token_secret = token_secret.into();
        if token_secret.trim().is_empty() {
            return Err(Error::Config("TOKEN_SECRET must not be empty".into()));
        }
        Ok(Self {
            database_path: database_path.into(),
            assets_dir: assets_dir.into(),
            token_secret,
            openai_base_url: "https://api.openai.com/v1".into(),
            openai_api_key: String::new(),
            openai_model: "gpt-4o-mini".into(),
        })
    }

    /// 从环境变量读取；DATABASE_PATH/ASSETS_DIR 有默认值，TOKEN_SECRET 必填。
    pub fn from_env() -> Result<Self> {
        let database_path =
            std::env::var("DATABASE_PATH").unwrap_or_else(|_| "shirita.db".into());
        let assets_dir = std::env::var("ASSETS_DIR").unwrap_or_else(|_| "./assets".into());
        let token_secret = std::env::var("TOKEN_SECRET")
            .map_err(|_| Error::Config("TOKEN_SECRET env var is required".into()))?;

        let mut cfg = Self::new(database_path, assets_dir, token_secret)?;
        if let Ok(v) = std::env::var("OPENAI_BASE_URL") {
            cfg.openai_base_url = v;
        }
        cfg.openai_api_key = std::env::var("OPENAI_API_KEY").unwrap_or_default();
        if let Ok(v) = std::env::var("OPENAI_MODEL") {
            cfg.openai_model = v;
        }
        Ok(cfg)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_rejects_empty_token() {
        let err = Config::new("db.sqlite", "./assets", "   ");
        assert!(err.is_err(), "empty/whitespace token must be rejected");
    }

    #[test]
    fn new_keeps_fields() {
        let cfg = Config::new("db.sqlite", "./assets", "secret").unwrap();
        assert_eq!(cfg.database_path, "db.sqlite");
        assert_eq!(cfg.assets_dir, "./assets");
        assert_eq!(cfg.token_secret, "secret");
    }
}
