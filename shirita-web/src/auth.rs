use axum::extract::{Request, State};
use axum::http::{header::AUTHORIZATION, StatusCode};
use axum::middleware::Next;
use axum::response::Response;

use crate::AppState;

/// Constant-time byte comparison: always walks the full length of `b` so the
/// time taken doesn't leak how many leading bytes of an attacker-supplied
/// token happened to match the real secret.
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

/// 校验 `Authorization: Bearer <token>` 是否等于配置中的静态 token（常量时间比较，
/// 防止通过响应耗时差异逐字节猜出 token）。
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
        Some(token) if constant_time_eq(token.as_bytes(), state.config.token_secret.as_bytes()) => {
            Ok(next.run(req).await)
        }
        _ => Err(StatusCode::UNAUTHORIZED),
    }
}

#[cfg(test)]
mod tests {
    use super::constant_time_eq;

    #[test]
    fn equal_strings_match() {
        assert!(constant_time_eq(b"secret-token", b"secret-token"));
    }

    #[test]
    fn different_content_does_not_match() {
        assert!(!constant_time_eq(b"secret-token", b"wrong-token!"));
    }

    #[test]
    fn different_length_does_not_match() {
        assert!(!constant_time_eq(b"short", b"a-much-longer-token"));
    }

    #[test]
    fn empty_strings_match() {
        assert!(constant_time_eq(b"", b""));
    }
}
