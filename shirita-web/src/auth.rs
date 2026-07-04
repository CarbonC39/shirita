use axum::extract::{Request, State};
use axum::http::{header::AUTHORIZATION, HeaderMap, HeaderName, HeaderValue, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use base64::Engine;

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

/// Verifies whether `Authorization: Bearer <token>` matches the static token in the configuration (a constant-time comparison,
/// to prevent the token from being guessed byte by byte based on differences in response times).
pub async fn require_bearer(
    State(state): State<AppState>,
    req: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    // When HTTP Basic Auth is also configured, the browser will auto-attach
    // `Authorization: Basic <creds>` to every same-origin request, which
    // `get(AUTHORIZATION)` sees first. Iterate all values so we pick up the
    // Bearer token that the frontend JS explicitly sets on fetch() calls.
    let provided = req
        .headers()
        .get_all(AUTHORIZATION)
        .iter()
        .filter_map(|h| h.to_str().ok())
        .find_map(|v| v.strip_prefix("Bearer "));

    match provided {
        Some(token) if constant_time_eq(token.as_bytes(), state.config.token_secret.as_bytes()) => {
            Ok(next.run(req).await)
        }
        _ => Err(StatusCode::UNAUTHORIZED),
    }
}

/// The realm string surfaced in the `WWW-Authenticate` challenge when Basic
/// auth is configured. Browsers show this label in their native login dialog.
const BASIC_REALM: &str = "Shirita";

/// 401 response with a `WWW-Authenticate: Basic realm="..."` challenge so the
/// browser pops its native login dialog. Returned by `require_basic` when no
/// (or wrong) credentials are supplied.
fn basic_challenge() -> Response {
    let mut headers = HeaderMap::new();
    // realm value is a static literal (no quotes), so it's a valid header value
    // without further escaping.
    let v = HeaderValue::from_str(&format!("Basic realm=\"{BASIC_REALM}\""))
        .expect("static realm string is a valid header value");
    headers.insert(
        HeaderName::from_static("www-authenticate"),
        v,
    );
    (StatusCode::UNAUTHORIZED, headers).into_response()
}

/// Optional HTTP Basic Auth gate applied as the outermost layer on the full
/// router (UI shell + /api + /assets + /health) for public deployments. When
/// `config.http_auth_user` / `http_auth_pass` are unset, this is a no-op so
/// desktop/local mode keeps its current behavior.
///
/// Credentials are compared in constant time to avoid byte-by-byte timing
/// leaks. Unlike Bearer (which protects /api only), this gates the served HTML
/// and assets too — without it, anyone reaching a public origin could pull the
/// UI shell and its JS even though every API call 401s.
//
// When the browser has cached Basic credentials it will auto-attach them to
// page-navigation requests (so the UI shell loads), but fetch() calls made by
// the loaded JS explicitly set `Authorization: Bearer <token>`, which
// *replaces* the browser's Basic header — the browser does not send both.
// Without a fallback, API calls would face a fresh Basic challenge on every
// request, breaking the SPA. The fallback: if the request carries a *valid*
// Bearer token (the same static token that is already embedded in the
// Basic-protected HTML), let it through. An attacker never sees the token
// without first passing Basic auth, so this does not weaken the gate.
pub async fn require_basic(
    State(state): State<AppState>,
    req: Request,
    next: Next,
) -> Response {
    let (expected_user, expected_pass) = match (
        &state.config.http_auth_user,
        &state.config.http_auth_pass,
    ) {
        (Some(u), Some(p)) => (u, p),
        _ => return next.run(req).await, // disabled: no creds configured
    };

    // Parse `Authorization: Basic <base64>` → user:pass.
    let provided = req
        .headers()
        .get(AUTHORIZATION)
        .and_then(|h| h.to_str().ok())
        .and_then(|v| v.strip_prefix("Basic "))
        .and_then(|b64| {
            base64::engine::general_purpose::STANDARD
                .decode(b64.trim())
                .ok()
        });

    let basic_ok = match provided {
        Some(decoded) => constant_time_eq(
            &decoded,
            format!("{expected_user}:{expected_pass}").as_bytes(),
        ),
        None => false,
    };

    if basic_ok {
        return next.run(req).await;
    }

    // No (valid) Basic credentials — check whether the request carries the
    // known Bearer token. When the JS inside the already-Basic-authenticated
    // page makes fetch() calls it only sends the Bearer header (the browser
    // omits the cached Basic header because JS set Authorization explicitly).
    // Letting a valid Bearer token through here avoids re-challenging every API
    // call while preserving the gate for anonymous visitors.
    let bearer_ok = req
        .headers()
        .get_all(AUTHORIZATION)
        .iter()
        .filter_map(|h| h.to_str().ok())
        .any(|v| {
            v.strip_prefix("Bearer ")
                .is_some_and(|token| constant_time_eq(token.as_bytes(), state.config.token_secret.as_bytes()))
        });

    if bearer_ok {
        return next.run(req).await;
    }

    basic_challenge()
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
