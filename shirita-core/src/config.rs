//! Runtime configuration: DATABASE_PATH / ASSETS_DIR / SHIRITA_BOOTSTRAP_*.

use crate::Result;

pub struct Config {
    pub database_path: String,
    pub assets_dir: String,
    pub openai_base_url: String,
    pub openai_api_key: String,
    pub openai_model: String,
    /// Optional first-account credentials for the bootstrap seeder, read from
    /// `SHIRITA_BOOTSTRAP_USER` / `SHIRITA_BOOTSTRAP_PASSWORD`. When unset, the
    /// seeder auto-generates an `admin` account on first start (web logs the
    /// random password; desktop leaves `password_hash` empty until the user sets
    /// one in Settings). Both are `None` for the desktop entry point.
    pub bootstrap_user: Option<String>,
    pub bootstrap_password: Option<String>,
}

impl Config {
    pub fn new(
        database_path: impl Into<String>,
        assets_dir: impl Into<String>,
    ) -> Result<Self> {
        Ok(Self {
            database_path: database_path.into(),
            assets_dir: assets_dir.into(),
            openai_base_url: "https://api.openai.com/v1".into(),
            openai_api_key: String::new(),
            openai_model: "gpt-4o-mini".into(),
            bootstrap_user: None,
            bootstrap_password: None,
        })
    }

    /// Read from environment variables; DATABASE_PATH and ASSETS_DIR have defaults.
    pub fn from_env() -> Result<Self> {
        let database_path =
            std::env::var("DATABASE_PATH").unwrap_or_else(|_| "shirita.db".into());
        let assets_dir = std::env::var("ASSETS_DIR").unwrap_or_else(|_| "./assets".into());

        let mut cfg = Self::new(database_path, assets_dir)?;
        apply_provider_env(&mut cfg);
        apply_bootstrap_env(&mut cfg);
        Ok(cfg)
    }
}

/// Reads optional `SHIRITA_BOOTSTRAP_USER` / `SHIRITA_BOOTSTRAP_PASSWORD`. A
/// blank value is treated as unset; a partial config (only one set) is ignored
/// so a stray empty password never creates an account that can't be logged into.
pub fn apply_bootstrap_env(cfg: &mut Config) {
    let user = std::env::var("SHIRITA_BOOTSTRAP_USER").ok().filter(|s| !s.is_empty());
    let pass = std::env::var("SHIRITA_BOOTSTRAP_PASSWORD").ok().filter(|s| !s.is_empty());
    // Gate on both being present; partial config → ignore (avoids a misconfig
    // where a blank password would seed an unusable account).
    match (user, pass) {
        (Some(u), Some(p)) => {
            cfg.bootstrap_user = Some(u);
            cfg.bootstrap_password = Some(p);
        }
        _ => {
            cfg.bootstrap_user = None;
            cfg.bootstrap_password = None;
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
    fn new_keeps_fields() {
        let cfg = Config::new("db.sqlite", "./assets").unwrap();
        assert_eq!(cfg.database_path, "db.sqlite");
        assert_eq!(cfg.assets_dir, "./assets");
        assert!(cfg.bootstrap_user.is_none());
        assert!(cfg.bootstrap_password.is_none());
    }

    #[test]
    fn apply_provider_env_overlays_openai_fields() {
        // SAFETY: Set up/clean up env within a single-threaded test.
        std::env::set_var("OPENAI_BASE_URL", "http://x/v1");
        std::env::set_var("OPENAI_MODEL", "m-test");
        let mut cfg = Config::new("db", "assets").unwrap();
        apply_provider_env(&mut cfg);
        assert_eq!(cfg.openai_base_url, "http://x/v1");
        assert_eq!(cfg.openai_model, "m-test");
        std::env::remove_var("OPENAI_BASE_URL");
        std::env::remove_var("OPENAI_MODEL");
    }

    #[test]
    fn apply_bootstrap_env_sets_creds_when_both_present() {
        // SAFETY: Set up/clean up env within a single-threaded test.
        std::env::set_var("SHIRITA_BOOTSTRAP_USER", "alice");
        std::env::set_var("SHIRITA_BOOTSTRAP_PASSWORD", "s3cret");
        let mut cfg = Config::new("db", "assets").unwrap();
        apply_bootstrap_env(&mut cfg);
        assert_eq!(cfg.bootstrap_user.as_deref(), Some("alice"));
        assert_eq!(cfg.bootstrap_password.as_deref(), Some("s3cret"));
        std::env::remove_var("SHIRITA_BOOTSTRAP_USER");
        std::env::remove_var("SHIRITA_BOOTSTRAP_PASSWORD");
    }

    #[test]
    fn apply_bootstrap_env_ignores_partial_config() {
        // SAFETY: single-threaded test. Only the user is set → must NOT seed
        // (a blank password would create an unusable account).
        std::env::set_var("SHIRITA_BOOTSTRAP_USER", "alice");
        std::env::remove_var("SHIRITA_BOOTSTRAP_PASSWORD");
        let mut cfg = Config::new("db", "assets").unwrap();
        apply_bootstrap_env(&mut cfg);
        assert!(cfg.bootstrap_user.is_none());
        assert!(cfg.bootstrap_password.is_none());
        std::env::remove_var("SHIRITA_BOOTSTRAP_USER");
    }

    #[test]
    fn apply_bootstrap_env_unset_disables_bootstrap() {
        // SAFETY: single-threaded test. Neither var set → None.
        std::env::remove_var("SHIRITA_BOOTSTRAP_USER");
        std::env::remove_var("SHIRITA_BOOTSTRAP_PASSWORD");
        let mut cfg = Config::new("db", "assets").unwrap();
        apply_bootstrap_env(&mut cfg);
        assert!(cfg.bootstrap_user.is_none());
        assert!(cfg.bootstrap_password.is_none());
    }
}
