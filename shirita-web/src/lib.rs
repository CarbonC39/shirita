//! shirita-web: Axum 适配层（REST + SSE + 静态文件 + 鉴权）

pub mod auth;
pub mod routes;
pub mod state;

pub use state::AppState;

use axum::{middleware, routing::get, Router};

/// 构建应用路由。`/health` 公开；`/api/*` 走 Bearer 中间件。
pub fn app(state: AppState) -> Router {
    let protected = Router::new()
        .route("/ping", get(routes::ping::ping))
        .route_layer(middleware::from_fn_with_state(
            state.clone(),
            auth::require_bearer,
        ));

    Router::new()
        .route("/health", get(routes::health::health))
        .nest("/api", protected)
        .with_state(state)
}
