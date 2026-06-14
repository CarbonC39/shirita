//! shirita-web: Axum 适配层（REST + SSE + 静态文件 + 鉴权）

pub mod auth;
pub mod routes;
pub mod state;

pub use state::AppState;

use axum::routing::{delete, post, put};
use axum::{middleware, routing::get, Router};
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
        .route("/sessions/{id}", delete(routes::sessions::delete_session))
        .route("/sessions/{id}/duplicate", post(routes::sessions::duplicate_session))
        .route("/sessions/{id}/export", get(routes::sessions::export_session))
        .route(
            "/sessions/{id}/messages",
            get(routes::sessions::list_messages).post(routes::chat::send),
        )
        .route("/sessions/{id}/mounts", put(routes::sessions::set_mounts))
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
        .route("/templates", get(routes::templates::list).post(routes::templates::create))
        .route("/templates/{id}", get(routes::templates::get).put(routes::templates::update).delete(routes::templates::delete))
        .route("/templates/{id}/duplicate", post(routes::templates::duplicate))
        .route("/templates/{id}/nodes", get(routes::prompt_nodes::list_nodes).post(routes::prompt_nodes::create_node))
        .route("/nodes/{id}", put(routes::prompt_nodes::update_node).delete(routes::prompt_nodes::delete_node))
        .route("/templates/{id}/nodes/reorder", put(routes::prompt_nodes::reorder_nodes))
        .route("/sessions/{id}/overrides", get(routes::overrides::list_overrides))
        .route("/sessions/{id}/overrides/{def_id}", put(routes::overrides::set_override).delete(routes::overrides::reset_override))
        .route("/sessions/{id}/overrides/{def_id}/promote", post(routes::overrides::promote_override))
        .route("/types", get(routes::types::list).post(routes::types::create))
        .route("/types/{id}", delete(routes::types::delete))
        .route("/import/worldinfo", post(routes::import_export::import_worldinfo))
        .route("/import/charcard", post(routes::import_export::import_charcard))
        .route("/settings", get(routes::settings::get_all).put(routes::settings::update_all))
        .route("/provider/test", post(routes::provider::test_connection))
        .route("/provider/models", get(routes::provider::list_models))
        .route("/assets", post(routes::assets::upload))
        .route_layer(middleware::from_fn_with_state(
            state.clone(),
            auth::require_bearer,
        ));

    Router::new()
        .route("/", get(routes::index::index))
        .route("/health", get(routes::health::health))
        .nest("/api", protected)
        .nest_service("/assets", ServeDir::new(assets_dir))
        .with_state(state)
}
