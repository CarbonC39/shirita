#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use shirita_core::{Config, SqliteStorage, Storage, TiktokenCounter, TokenCounter};
use shirita_web::{app_with_cors, AppState, Generations};
use sqlx::SqlitePool;
use tauri::{Manager, RunEvent, WebviewUrl, WebviewWindowBuilder};
use tauri_plugin_dialog::{DialogExt, MessageDialogKind};
use tokio_util::sync::CancellationToken;

fn data_paths(base: &Path) -> (PathBuf, PathBuf) {
    (base.join("shirita.db"), base.join("assets"))
}

struct Shutdown {
    token: CancellationToken,
    pool: SqlitePool,
}

async fn boot(base: PathBuf) -> Result<(AppState, SqlitePool, u16, CancellationToken, String), String> {
    std::fs::create_dir_all(&base)
        .map_err(|e| format!("Failed to create data directory {}: {e}", base.display()))?;
    let (db_path, assets_dir) = data_paths(&base);
    std::fs::create_dir_all(&assets_dir).map_err(|e| format!("Failed to create assets directory: {e}"))?;

    let token_secret = uuid::Uuid::new_v4().to_string();
    let mut config = Config::new(
        db_path.to_string_lossy().to_string(),
        assets_dir.to_string_lossy().to_string(),
        &token_secret,
    )
    .map_err(|e| format!("Configuration error: {e}"))?;
    shirita_core::apply_provider_env(&mut config);
    let model = config.openai_model.clone();

    let storage = SqliteStorage::connect(&config.database_path)
        .await
        .map_err(|e| format!("Failed to connect to the database: {e}"))?;
    storage
        .run_migrations()
        .await
        .map_err(|e| format!("Database migration failed: {e}"))?;
    shirita_core::ensure_default_template(&storage)
        .await
        .map_err(|e| format!("Failed to initialize default templates: {e}"))?;
    shirita_core::ensure_templates_have_content_node(&storage)
        .await
        .map_err(|e| format!("Failed to migrate template content nodes: {e}"))?;
    shirita_core::ensure_asset_hashes(&storage, &config.assets_dir)
        .await
        .map_err(|e| format!("Failed to backfill asset hashes: {e}"))?;
    let pool = storage.pool().clone();

    let http_client = shirita_web::new_http_client();
    let provider = shirita_web::provider_from_env(&config, http_client.clone());
    let storage: Arc<dyn Storage> = Arc::new(storage);
    let token_counter: Arc<dyn TokenCounter> = Arc::new(TiktokenCounter::new());
    let state = AppState {
        storage,
        config: Arc::new(config),
        provider,
        token_counter,
        model,
        generations: Arc::new(Generations::new()),
        http_client,
    };

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .map_err(|e| {
            format!("Failed to bind to local port: {e}\n\nPlease check your local firewall or antivirus block logs.")
        })?;
    let port = listener
        .local_addr()
        .map_err(|e| format!("Failed to read local port: {e}"))?
        .port();

    let token = CancellationToken::new();
    let child = token.clone();
    let serve_state = state.clone();

    tauri::async_runtime::spawn(async move {
        if let Err(e) = axum::serve(listener, app_with_cors(serve_state))
            .with_graceful_shutdown(async move { child.cancelled().await })
            .await
        {
            tracing::error!("embedded server error: {e}");
        }
    });

    Ok((state, pool, port, token, token_secret))
}

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .init();

    let app = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let handle = app.handle().clone();
            let base = handle
                .path()
                .app_data_dir()
                .map_err(|e| format!("Failed to locate data directory: {e}"))?;

            let boot_result = tauri::async_runtime::block_on(boot(base));
            let (_state, pool, port, token, token_secret) = match boot_result {
                Ok(v) => v,
                Err(msg) => {
                    handle
                        .dialog()
                        .message(msg)
                        .kind(MessageDialogKind::Error)
                        .title("Shirita Startup Failed")
                        .blocking_show();
                    std::process::exit(1);
                }
            };

            let runtime_cfg = serde_json::json!({
                "base": format!("http://127.0.0.1:{port}"),
                "token": token_secret,
            });
            let init_script = format!("window.__SHIRITA_RUNTIME__ = {runtime_cfg};");

            WebviewWindowBuilder::new(&handle, "main", WebviewUrl::default())
                .title("Shirita")
                .inner_size(1100.0, 760.0)
                .initialization_script(init_script.as_str())
                .build()?;

            app.manage(Shutdown { token, pool });
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error while building tauri application");

    app.run(|app_handle, event| {
        if let RunEvent::Exit = event {
            if let Some(sd) = app_handle.try_state::<Shutdown>() {
                let token = sd.token.clone();
                let pool = sd.pool.clone();
                tauri::async_runtime::block_on(async move {
                    token.cancel();
                    tokio::time::sleep(Duration::from_millis(50)).await;
                    pool.close().await;
                });
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn data_paths_derives_db_and_assets() {
        let (db, assets) = data_paths(Path::new("/data"));
        assert_eq!(db, Path::new("/data/shirita.db"));
        assert_eq!(assets, Path::new("/data/assets"));
    }
}
