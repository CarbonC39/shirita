//! Session-token authentication.
//!
//! [`require_session`] protects `/api/*`: an opaque bearer token is looked up
//! in the `sessions` table (with expiry), and the resolved user is attached as
//! an [`AuthUser`] request extension for downstream handlers. This is the
//! multi-user seam — every handler can read it, even though none filter by it
//! yet.
//!
//! [`require_asset_access`] does the same for `/assets/*` but additionally
//! accepts the token via a `?t=<token>` query parameter, so `<img src>` URLs
//! (which cannot set headers) can authenticate.

use axum::extract::{Request, State};
use axum::http::{header::AUTHORIZATION, HeaderMap, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use chrono::DateTime;

use crate::AppState;

/// The authenticated principal, inserted into request extensions by
/// `require_session`. Handlers extract it via `Extension<AuthUser>`.
#[derive(Debug, Clone)]
pub struct AuthUser {
    pub id: String,
    pub username: String,
}

/// Pull a `Bearer <token>` out of the (possibly multiple) Authorization headers.
/// All values are scanned rather than taking the first, so a stray header does
/// not shadow the Bearer the frontend JS sets on its `fetch()` calls.
pub(crate) fn bearer_token(headers: &HeaderMap) -> Option<&str> {
    headers
        .get_all(AUTHORIZATION)
        .iter()
        .filter_map(|h| h.to_str().ok())
        .find_map(|v| v.strip_prefix("Bearer "))
}

/// Resolve a token to a user, enforcing expiry. Shared by both middlewares so
/// the header (`require_session`) and query-string (`require_asset_access`)
/// paths use identical validation.
async fn resolve_session(state: &AppState, token: &str) -> Option<AuthUser> {
    let session = state.storage.get_auth_session(token).await.ok()??;
    // Expiry: an unparseable timestamp is treated as invalid (never "valid").
    let expires_at = DateTime::parse_from_rfc3339(&session.expires_at).ok()?;
    if expires_at <= chrono::Utc::now() {
        return None;
    }
    let user = state.storage.get_user(&session.user_id).await.ok()??;
    Some(AuthUser { id: user.id, username: user.username })
}

/// Middleware: require a valid non-expired bearer session token on `/api/*`.
pub async fn require_session(
    State(state): State<AppState>,
    mut req: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    let Some(token) = bearer_token(req.headers()) else {
        return Err(StatusCode::UNAUTHORIZED);
    };
    let Some(user) = resolve_session(&state, token).await else {
        return Err(StatusCode::UNAUTHORIZED);
    };
    req.extensions_mut().insert(user);
    Ok(next.run(req).await)
}

/// Middleware: gate `/assets/*` behind a valid session presented either as a
/// `Bearer` header or as a `?t=<token>` query parameter (the latter lets
/// `<img>` URLs authenticate). Unauthorized → bodyless 401.
pub async fn require_asset_access(
    State(state): State<AppState>,
    req: Request,
    next: Next,
) -> Response {
    let header_tok = bearer_token(req.headers()).map(str::to_owned);
    let query_tok = req.uri().query().and_then(|q| {
        q.split('&').filter_map(|kv| {
            let (k, v) = kv.split_once('=')?;
            (k == "t").then_some(v.to_owned())
        }).next()
    });
    let token = header_tok.or(query_tok);
    let allowed = match token {
        Some(ref t) => resolve_session(&state, t).await.is_some(),
        None => false,
    };
    if allowed {
        next.run(req).await
    } else {
        StatusCode::UNAUTHORIZED.into_response()
    }
}

/// Render a request URI with the `t` (session-token) query parameter redacted.
/// Other params are preserved. Used by the request-logging `TraceLayer` so the
/// long-lived bearer token — which appears in `/assets/<file>?t=<token>` — is
/// never written to the terminal or `docker logs`.
pub fn redact_query(uri: &axum::http::Uri) -> String {
    let path = uri.path();
    match uri.query() {
        None => path.to_string(),
        Some(q) => {
            let filtered: Vec<String> = q
                .split('&')
                .map(|kv| {
                    let (k, _v) = kv.split_once('=').unwrap_or((kv, ""));
                    if k == "t" { "t=<redacted>".to_string() } else { kv.to_string() }
                })
                .collect();
            if filtered.is_empty() {
                path.to_string()
            } else {
                format!("{path}?{}", filtered.join("&"))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::Uri;

    #[test]
    fn bearer_token_skips_non_bearer_headers() {
        let mut h = HeaderMap::new();
        h.insert(AUTHORIZATION, "Basic dXNlcjpwYXNz".parse().unwrap());
        h.append(AUTHORIZATION, "Bearer abc.def".parse().unwrap());
        assert_eq!(bearer_token(&h), Some("abc.def"));
    }

    #[test]
    fn redact_query_keeps_other_params_and_scrubs_t() {
        let uri: Uri = "/assets/x.png?size=large&t=SECRET&v=2".parse().unwrap();
        assert_eq!(redact_query(&uri), "/assets/x.png?size=large&t=<redacted>&v=2");
    }

    #[test]
    fn redact_query_drops_query_when_only_t_present_value_redacted() {
        let uri: Uri = "/assets/x.png?t=SECRET".parse().unwrap();
        assert_eq!(redact_query(&uri), "/assets/x.png?t=<redacted>");
    }

    #[test]
    fn redact_query_passthrough_when_no_query() {
        let uri: Uri = "/api/sessions".parse().unwrap();
        assert_eq!(redact_query(&uri), "/api/sessions");
    }
}
