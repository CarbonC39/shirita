use std::sync::Arc;

use shirita_core::{Config, SqliteStorage, Storage, TiktokenCounter, TokenCounter};
use shirita_web::{app, new_http_client, provider_from_env, AppState};

/// Is `addr` (a `BIND_ADDR`-style `host:port` string) loopback-only?
/// The web build requires login, but the login endpoint and `/assets` are still
/// public and login has no rate-limiting — binding beyond loopback exposes a
/// brute-forceable surface unless the deployer fronts it with a reverse proxy.
fn is_loopback_bind(addr: &str) -> bool {
    let host = addr.rsplit_once(':').map(|(h, _)| h).unwrap_or(addr);
    let host = host.trim_start_matches('[').trim_end_matches(']');
    host == "localhost" || host.parse::<std::net::IpAddr>().is_ok_and(|ip| ip.is_loopback())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info".into()),
        )
        .init();

    let config = Config::from_env()?;
    let storage = SqliteStorage::connect(&config.database_path).await?;
    storage.run_migrations().await?;
    // First-launch convenience: seed a default template if none exist yet.
    shirita_core::ensure_default_template(&storage).await?;
    shirita_core::ensure_builtin_definitions(&storage).await?;
    // Backfill: legacy templates gain the undeletable <<content>> mount node.
    shirita_core::ensure_templates_have_content_node(&storage).await?;
    // Backfill: unreferenced regex_rule defs that predate the is_global flag.
    shirita_core::ensure_global_regex_flag(&storage).await?;
    shirita_core::ensure_asset_hashes(&storage, &config.assets_dir).await?;
    // Provision the first account: SHIRITA_BOOTSTRAP_* if set, else auto-generate
    // (a real password, logged once below; change it in Settings).
    let bootstrap = shirita_core::BootstrapCreds {
        user: config.bootstrap_user.clone(),
        password: config.bootstrap_password.clone(),
        interactive: true,
    };
    if let Some(creds) = shirita_core::ensure_bootstrap_user(&storage, &bootstrap).await? {
        tracing::info!(
            "No account found — created initial '{}' account. Generated password: {} \
             (log in and change it in Settings, or set SHIRITA_BOOTSTRAP_USER / \
             SHIRITA_BOOTSTRAP_PASSWORD to provision silently).",
            creds.username,
            creds.password,
        );
    }
    tokio::fs::create_dir_all(&config.assets_dir).await?;

    // Shared HTTP client: The fallback provider (based on environment variables) and the runtime settings provider share the same connection pool.
    let http_client = new_http_client();
    // Select the fallback adapter based on the PROVIDER environment variable (defaults to OpenAI-compatible; falls back to offline Echo if no key is provided).
    let provider = provider_from_env(&config, http_client.clone());

    let model = config.openai_model.clone();
    let storage: Arc<dyn Storage> = Arc::new(storage);
    let token_counter: Arc<dyn TokenCounter> = Arc::new(TiktokenCounter::new());
    let state = AppState {
        storage,
        config: Arc::new(config),
        provider,
        token_counter,
        model,
        generations: Arc::new(shirita_web::Generations::new()),
        http_client,
    };

    let bind_addr = std::env::var("BIND_ADDR").unwrap_or_else(|_| "127.0.0.1:8787".into());
    if !is_loopback_bind(&bind_addr) {
        tracing::warn!(
            "BIND_ADDR={bind_addr} is not loopback-only: the app is reachable on \
             the public network. The login endpoint and /assets are public and \
             login has no rate-limiting, so brute-force is possible — put this \
             behind your own reverse proxy / rate-limiter, or bind to 127.0.0.1."
        );
    }
    let listener = tokio::net::TcpListener::bind(&bind_addr).await?;
    tracing::info!("shirita-web listening on {bind_addr}");
    axum::serve(listener, app(state)).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::is_loopback_bind;

    #[test]
    fn recognizes_loopback_addresses() {
        assert!(is_loopback_bind("127.0.0.1:8787"));
        assert!(is_loopback_bind("localhost:8787"));
        assert!(is_loopback_bind("[::1]:8787"));
    }

    #[test]
    fn rejects_non_loopback_addresses() {
        assert!(!is_loopback_bind("0.0.0.0:8787"));
        assert!(!is_loopback_bind("192.168.1.10:8787"));
        assert!(!is_loopback_bind("[::]:8787"));
    }
}
