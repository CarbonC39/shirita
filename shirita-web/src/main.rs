use std::sync::Arc;

use shirita_core::{
    Config, EchoProvider, ModelProvider, OpenAiProvider, SqliteStorage, Storage, TiktokenCounter,
    TokenCounter,
};
use shirita_web::{app, AppState};

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

    // 无 API key → 离线 Echo；有 key → 真实 OpenAI 兼容接口。
    let provider: Arc<dyn ModelProvider> = if config.openai_api_key.is_empty() {
        tracing::info!("OPENAI_API_KEY empty: using offline EchoProvider");
        Arc::new(EchoProvider)
    } else {
        tracing::info!("using OpenAiProvider at {}", config.openai_base_url);
        Arc::new(OpenAiProvider::new(
            config.openai_base_url.clone(),
            config.openai_api_key.clone(),
        ))
    };

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
    };

    let bind_addr = std::env::var("BIND_ADDR").unwrap_or_else(|_| "127.0.0.1:8787".into());
    let listener = tokio::net::TcpListener::bind(&bind_addr).await?;
    tracing::info!("shirita-web listening on {bind_addr}");
    axum::serve(listener, app(state)).await?;
    Ok(())
}
