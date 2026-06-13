use axum::extract::{Request, State};
use axum::http::{header::AUTHORIZATION, StatusCode};
use axum::middleware::Next;
use axum::response::Response;

use crate::AppState;

/// 校验 `Authorization: Bearer <token>` 是否等于配置中的静态 token。
/// MVP 阶段为简单字符串比较；链路必须存在（后续可替换为更强校验）。
pub async fn require_bearer(
    State(state): State<AppState>,
    req: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    let provided = req
        .headers()
        .get(AUTHORIZATION)
        .and_then(|h| h.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "));

    match provided {
        Some(token) if token == state.config.token_secret => Ok(next.run(req).await),
        _ => Err(StatusCode::UNAUTHORIZED),
    }
}
