//! shirita-web: Axum 适配层（REST + SSE + 静态文件 + 鉴权）

pub mod auth;
pub mod routes;
pub mod state;

pub use state::AppState;

use axum::routing::{post, put};
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
