//! Runtime configuration: DATABASE_PATH / ASSETS_DIR / TOKEN_SECRET.

use crate::{Error, Result};

pub struct Config {
    pub database_path: String,
    pub assets_dir: String,
    pub token_secret: String,
    pub openai_base_url: String,
    pub openai_api_key: String,
    pub openai_model: String,
    /// Optional HTTP Basic Auth credentials for public deployments. When set,
    /// the entire app (UI shell + /api + /assets + /health) is gated behind a
    /// browser-native login. Bearer-token auth only protects /api, so on a
    /// public origin the UI HTML/JS would otherwise be served to anyone — this
    /// layer closes that gap. `None` when neither env var is set (desktop/local
    /// mode keeps current behavior).
    pub http_auth_user: Option<String>,
    pub http_auth_pass: Option<String>,
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
            http_auth_user: None,
            http_auth_pass: None,
        })
    }

    /// Read from environment variables; DATABASE_PATH and ASSETS_DIR have default values, while TOKEN_SECRET is mandatory.
    pub fn from_env() -> Result<Self> {
        let database_path =
            std::env::var("DATABASE_PATH").unwrap_or_else(|_| "shirita.db".into());
        let assets_dir = std::env::var("ASSETS_DIR").unwrap_or_else(|_| "./assets".into());
        let token_secret = std::env::var("TOKEN_SECRET")
            .map_err(|_| Error::Config("TOKEN_SECRET env var is required".into()))?;

        let mut cfg = Self::new(database_path, assets_dir, token_secret)?;
        apply_provider_env(&mut cfg);
        apply_http_auth_env(&mut cfg);
        Ok(cfg)
    }
}

/// Reads optional HTTP_AUTH_USER / HTTP_AUTH_PASS. Both must be set to enable
/// Basic auth; setting only one has no effect (avoids a misconfig where a blank
/// password would gate nothing meaningful), so an unset/blank field clears both.
pub fn apply_http_auth_env(cfg: &mut Config) {
    let user = std::env::var("HTTP_AUTH_USER").ok().filter(|s| !s.is_empty());
    let pass = std::env::var("HTTP_AUTH_PASS").ok().filter(|s| !s.is_empty());
    // Gate on both being present; partial config → disable auth entirely.
    match (user, pass) {
        (Some(u), Some(p)) => {
            cfg.http_auth_user = Some(u);
            cfg.http_auth_pass = Some(p);
        }
        _ => {
            cfg.http_auth_user = None;
            cfg.http_auth_pass = None;
        }
    }
}

/// Merges provider-related environment variables (OPENAI_BASE_URL, OPENAI_API_KEY, OPENAI_MODEL) into the configuration.
/// Shared by `from_env` and the desktop (Tauri) entry point to avoid duplication.
pub fn apply_provider_env(cfg: &mut Config) {
    if let Ok(v) = std::env::var("OPENAI_BASE_URL") {
        cfg.openai_base_url = v;
    }
    cfg.openai_api_key = std::env::var("OPENAI_API_KEY").unwrap_or_default();
    if let Ok(v) = std::env::var("OPENAI_MODEL") {
        cfg.openai_model = v;
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

    #[test]
    fn apply_provider_env_overlays_openai_fields() {
        // SAFETY: Set up/clean up env within a single-threaded test.
        std::env::set_var("OPENAI_BASE_URL", "http://x/v1");
        std::env::set_var("OPENAI_MODEL", "m-test");
        let mut cfg = Config::new("db", "assets", "tok").unwrap();
        apply_provider_env(&mut cfg);
        assert_eq!(cfg.openai_base_url, "http://x/v1");
        assert_eq!(cfg.openai_model, "m-test");
        std::env::remove_var("OPENAI_BASE_URL");
        std::env::remove_var("OPENAI_MODEL");
    }

    #[test]
    fn apply_http_auth_env_sets_creds_when_both_present() {
        // SAFETY: Set up/clean up env within a single-threaded test.
        std::env::set_var("HTTP_AUTH_USER", "alice");
        std::env::set_var("HTTP_AUTH_PASS", "s3cret");
        let mut cfg = Config::new("db", "assets", "tok").unwrap();
        apply_http_auth_env(&mut cfg);
        assert_eq!(cfg.http_auth_user.as_deref(), Some("alice"));
        assert_eq!(cfg.http_auth_pass.as_deref(), Some("s3cret"));
        std::env::remove_var("HTTP_AUTH_USER");
        std::env::remove_var("HTTP_AUTH_PASS");
    }

    #[test]
    fn apply_http_auth_env_ignores_partial_config() {
        // SAFETY: single-threaded test. Only the user is set → must NOT enable
        // auth (a blank password would gate nothing meaningful).
        std::env::set_var("HTTP_AUTH_USER", "alice");
        std::env::remove_var("HTTP_AUTH_PASS");
        let mut cfg = Config::new("db", "assets", "tok").unwrap();
        apply_http_auth_env(&mut cfg);
        assert!(cfg.http_auth_user.is_none());
        assert!(cfg.http_auth_pass.is_none());
        std::env::remove_var("HTTP_AUTH_USER");
    }

    #[test]
    fn apply_http_auth_env_unset_disables_auth() {
        // SAFETY: single-threaded test. Neither var set → disabled.
        std::env::remove_var("HTTP_AUTH_USER");
        std::env::remove_var("HTTP_AUTH_PASS");
        let mut cfg = Config::new("db", "assets", "tok").unwrap();
        apply_http_auth_env(&mut cfg);
        assert!(cfg.http_auth_user.is_none());
        assert!(cfg.http_auth_pass.is_none());
    }
}
