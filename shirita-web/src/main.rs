use std::sync::Arc;

use shirita_core::{Config, SqliteStorage, Storage, TiktokenCounter, TokenCounter};
use shirita_web::{app, new_http_client, provider_from_env, AppState};

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
    tokio::fs::create_dir_all(&config.assets_dir).await.ok();

    // 共享 HTTP 客户端：env 兜底 provider 与运行期 settings provider 复用同一连接池。
    let http_client = new_http_client();
    // 按 PROVIDER env 选择兜底适配器（默认 OpenAI 兼容；无 key 则离线 Echo）。
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
    let listener = tokio::net::TcpListener::bind(&bind_addr).await?;
    tracing::info!("shirita-web listening on {bind_addr}");
    axum::serve(listener, app(state)).await?;
    Ok(())
}
