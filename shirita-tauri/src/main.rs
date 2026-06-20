#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::path::{Path, PathBuf};
use std::sync::Arc;

use shirita_core::{Config, SqliteStorage, Storage, TiktokenCounter, TokenCounter};
use shirita_web::{app_with_cors, AppState, Generations};
use sqlx::SqlitePool;
use tauri::{Manager, RunEvent, WebviewUrl, WebviewWindowBuilder};
use tauri_plugin_dialog::{DialogExt, MessageDialogKind};
use tokio_util::sync::CancellationToken;

/// 由 app data 基目录推出 (db_path, assets_dir)。纯函数，便于单测。
fn data_paths(base: &Path) -> (PathBuf, PathBuf) {
    (base.join("shirita.db"), base.join("assets"))
}

/// 退出时优雅关闭所需的句柄（managed state）。
struct Shutdown {
    token: CancellationToken,
    pool: SqlitePool,
}

/// 启动序列里所有可失败步骤集中于此，返回人类可读错误供对话框展示。
async fn boot(base: PathBuf) -> Result<(AppState, SqlitePool, u16, CancellationToken, String), String> {
    std::fs::create_dir_all(&base)
        .map_err(|e| format!("无法创建数据目录 {}：{e}", base.display()))?;
    let (db_path, assets_dir) = data_paths(&base);
    std::fs::create_dir_all(&assets_dir).map_err(|e| format!("无法创建资源目录：{e}"))?;

    let token_secret = uuid::Uuid::new_v4().to_string();
    let mut config = Config::new(
        db_path.to_string_lossy().to_string(),
        assets_dir.to_string_lossy().to_string(),
        &token_secret,
    )
    .map_err(|e| format!("配置错误：{e}"))?;
    shirita_core::apply_provider_env(&mut config);
    let model = config.openai_model.clone();

    let storage = SqliteStorage::connect(&config.database_path)
        .await
        .map_err(|e| format!("打开数据库失败：{e}"))?;
    storage
        .run_migrations()
        .await
        .map_err(|e| format!("数据库迁移失败：{e}"))?;
    shirita_core::ensure_default_template(&storage)
        .await
        .map_err(|e| format!("初始化默认模板失败：{e}"))?;
    shirita_core::ensure_templates_have_content_node(&storage)
        .await
        .map_err(|e| format!("迁移模板 content 节点失败：{e}"))?;
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
            format!("无法绑定本地端口：{e}\n\n请检查本地防火墙或杀毒软件的拦截记录。")
        })?;
    let port = listener
        .local_addr()
        .map_err(|e| format!("读取本地端口失败：{e}"))?
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
                .map_err(|e| format!("无法定位数据目录：{e}"))?;

            let boot_result = tauri::async_runtime::block_on(boot(base));
            let (_state, pool, port, token, token_secret) = match boot_result {
                Ok(v) => v,
                Err(msg) => {
                    handle
                        .dialog()
                        .message(msg)
                        .kind(MessageDialogKind::Error)
                        .title("Shirita 启动失败")
                        .blocking_show();
                    std::process::exit(1);
                }
            };

            // 注入运行时配置（在页面脚本之前执行）。
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
