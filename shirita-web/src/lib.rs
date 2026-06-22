//! shirita-web: Axum 适配层（REST + SSE + 静态文件 + 鉴权）

pub mod auth;
pub mod embed;
pub mod generations;
pub mod provider_select;
pub mod routes;
pub mod state;

pub use generations::Generations;
pub use provider_select::{
    provider_from_env, provider_kind, resolve_provider, resolve_provider_config, ProviderKind,
};
pub use state::AppState;

/// 构造全进程共享的 HTTP 客户端。两个入口（web/tauri）各调一次，存入 `AppState.http_client`。
pub fn new_http_client() -> reqwest::Client {
    reqwest::Client::new()
}

use axum::extract::DefaultBodyLimit;
use axum::http::{header, Method};
use axum::routing::{delete, post, put};
use axum::{middleware, routing::get, Router};
use tower_http::cors::{AllowOrigin, CorsLayer};
use tower_http::services::ServeDir;

/// 构建应用路由。`/`、`/health`、`GET /assets/*` 公开；`/api/*` 走 Bearer 中间件。
pub fn app(state: AppState) -> Router {
    let assets_dir = state.config.assets_dir.clone();

    let protected = Router::new()
        .route("/ping", get(routes::ping::ping))
        .route(
            "/sessions",
            get(routes::sessions::list_sessions).post(routes::sessions::create_session),
        )
        .route("/sessions/import", post(routes::sessions::import_session))
        .route("/sessions/reorder", put(routes::sessions::reorder_sessions))
        .route(
            "/sessions/{id}",
            get(routes::sessions::get_session)
                .patch(routes::sessions::patch_session)
                .delete(routes::sessions::delete_session),
        )
        .route("/sessions/{id}/duplicate", post(routes::sessions::duplicate_session))
        .route("/sessions/{id}/export", get(routes::sessions::export_session))
        .route("/sessions/{id}/identity", get(routes::sessions::get_session_identity))
        .route(
            "/sessions/{id}/messages",
            get(routes::sessions::list_messages).post(routes::chat::send),
        )
        .route(
            "/sessions/{id}/messages/{msg_id}",
            put(routes::messages::edit_message),
        )
        .route(
            "/sessions/{id}/active-leaf",
            put(routes::messages::set_active_leaf),
        )
        .route(
            "/sessions/{id}/messages/{msg_id}/regenerate",
            post(routes::chat::regenerate_message),
        )
        .route("/sessions/{id}/fork", post(routes::messages::fork_session))
        .route("/sessions/{id}/mounts", put(routes::sessions::set_mounts))
        .route("/sessions/{id}/packs", get(routes::sessions::get_packs).put(routes::sessions::set_packs))
        .route(
            "/sessions/{id}/local-definitions/{def_id}",
            put(routes::local_overrides::set_local_definition)
                .delete(routes::local_overrides::clear_local_definition),
        )
        .route(
            "/sessions/{id}/local-definitions/{def_id}/promote",
            post(routes::local_overrides::promote_local_definition),
        )
        .route(
            "/sessions/{id}/materialize-nodes",
            post(routes::local_overrides::materialize_nodes),
        )
        .route("/sessions/{id}/state", get(routes::variables::get_state))
        .route("/sessions/{id}/local-variables", put(routes::variables::set_local_variables))
        .route("/sessions/{id}/state-updates", post(routes::variables::apply_state_updates))
        .route(
            "/definitions",
            get(routes::definitions::list).post(routes::definitions::create),
        )
        .route(
            "/definitions/{id}",
            get(routes::definitions::get)
                .put(routes::definitions::update)
                .delete(routes::definitions::delete),
        )
        .route("/definitions/{id}/export", get(routes::export::export_definition))
        .route("/templates/{id}/export", get(routes::export::export_template))
        .route("/templates", get(routes::templates::list).post(routes::templates::create))
        .route("/templates/{id}", get(routes::templates::get).put(routes::templates::update).delete(routes::templates::delete))
        .route("/templates/{id}/duplicate", post(routes::templates::duplicate))
        .route("/templates/{id}/orphan-definitions", get(routes::templates::orphan_definitions))
        .route("/templates/{id}/nodes", get(routes::prompt_nodes::list_nodes).post(routes::prompt_nodes::create_node))
        .route("/nodes/{id}", put(routes::prompt_nodes::update_node).delete(routes::prompt_nodes::delete_node))
        .route("/templates/{id}/nodes/reorder", put(routes::prompt_nodes::reorder_nodes))
        .route("/packs", get(routes::packs::list).post(routes::packs::create))
        .route("/packs/{id}", get(routes::packs::get).put(routes::packs::update).delete(routes::packs::delete))
        .route("/packs/{id}/duplicate", post(routes::packs::duplicate))
        .route("/packs/{id}/orphan-definitions", get(routes::packs::orphan_definitions))
        .route("/packs/{id}/export", get(routes::export::export_pack))
        .route("/packs/{id}/nodes", get(routes::prompt_nodes::list_nodes).post(routes::prompt_nodes::create_node))
        .route("/packs/{id}/nodes/reorder", put(routes::prompt_nodes::reorder_nodes))
        .route("/types", get(routes::types::list).post(routes::types::create))
        .route("/types/{id}", delete(routes::types::delete))
        .route("/import/worldinfo", post(routes::import_export::import_worldinfo))
        .route("/import/charcard", post(routes::import_export::import_charcard))
        .route("/import", post(routes::import_export::import).layer(DefaultBodyLimit::max(16 * 1024 * 1024)))
        .route("/regex-rules/scopes", get(routes::regex_rules::list_regex_scopes))
        .route("/settings", get(routes::settings::get_all).put(routes::settings::update_all))
        .route("/provider/test", post(routes::provider::test_connection))
        .route("/provider/models", get(routes::provider::list_models))
        .route("/assets", get(routes::assets::list).post(routes::assets::upload))
        .route("/assets/{id}", put(routes::assets::rename).delete(routes::assets::delete))
        .route_layer(middleware::from_fn_with_state(
            state.clone(),
            auth::require_bearer,
        ));

    let router = Router::new()
        .route("/health", get(routes::health::health))
        .nest("/api", protected)
        .nest_service("/assets", ServeDir::new(assets_dir));

    #[cfg(feature = "embed-ui")]
    let router = router
        .route("/", get(embed::serve_index))
        .route("/static/{*path}", get(embed::serve_static))
        .fallback(embed::spa_fallback);

    #[cfg(not(feature = "embed-ui"))]
    let router = router.route("/", get(routes::index::index));

    router.with_state(state)
}

/// 桌面 webview 的 origin：
/// - 生产（`tauri build`，custom-protocol）：`tauri://localhost`（Linux/macOS）/
///   `https://tauri.localhost`（Windows）；
/// - 开发（`tauri dev`，加载 devUrl）：`http://localhost:<port>`。
/// 内嵌 server 绑 127.0.0.1 且受 Bearer 守护，故放行 localhost/127.0.0.1 任意端口是安全的。
fn is_desktop_origin(origin: &header::HeaderValue) -> bool {
    let o = origin.as_bytes();
    o == b"tauri://localhost"
        || o == b"https://tauri.localhost"
        || o == b"http://tauri.localhost"
        || o.starts_with(b"http://localhost:")
        || o.starts_with(b"http://127.0.0.1:")
}

/// 桌面（内嵌 server）专用：在 `app()` 外层套 CORS，放行 Tauri webview origin。
/// CorsLayer 作为最外层——preflight `OPTIONS` 由它短路应答，不进 Bearer 鉴权；
/// 真实请求穿过 CORS → auth → handler，响应回程补上 `Access-Control-Allow-Origin`。
pub fn app_with_cors(state: AppState) -> Router {
    let cors = CorsLayer::new()
        .allow_origin(AllowOrigin::predicate(|origin, _req| is_desktop_origin(origin)))
        .allow_methods([
            Method::GET,
            Method::POST,
            Method::PUT,
            Method::DELETE,
            Method::OPTIONS,
        ])
        .allow_headers([header::AUTHORIZATION, header::CONTENT_TYPE]);
    app(state).layer(cors)
}
