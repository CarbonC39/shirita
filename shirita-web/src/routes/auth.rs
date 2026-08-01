//! Authentication endpoints: login (public), logout / me / change-password
//! (session-protected — see `lib::app` wiring).
//!
//! The login dummy-hash mitigation: when the username is unknown we still run
//! argon2 verification against a cached dummy hash, so the 401 latency is
//! indistinguishable from a wrong-password attempt (no user enumeration via
//! timing). The dummy is generated once at first use with the *same* `Params`
//! and salt path as real hashes, so the cost is identical.

use std::sync::OnceLock;

use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::Json;
use chrono::Duration;
use serde::{Deserialize, Serialize};
use shirita_core::AuthSessionRecord;

use crate::auth::{bearer_token, AuthUser};
use crate::AppState;

/// Web session lifetime. Sliding renewal would write on every request; a fixed
/// TTL is cheaper and sufficient. Desktop auto-sessions use a longer lifetime
/// (see the Tauri boot).
const SESSION_DAYS: i64 = 30;

/// The fixed string hashed to produce the login dummy hash. Its plaintext is
/// irrelevant — it just must hash with the real `Params`.
const DUMMY_SECRET: &str = "shirita-login-dummy-do-not-use";

/// A cached argon2 hash of [`DUMMY_SECRET`], generated once with the real
/// hasher so unknown-username logins cost the same as wrong-password logins.
static DUMMY_HASH: OnceLock<String> = OnceLock::new();
fn dummy_hash() -> &'static str {
    DUMMY_HASH.get_or_init(|| {
        shirita_core::hash_password(DUMMY_SECRET).unwrap_or_else(|e| {
            tracing::error!(
                "failed to build login dummy hash ({e}); user-enumeration timing \
                 mitigation is disabled until the argon2 config is fixed"
            );
            String::new()
        })
    })
}

#[derive(Debug, Deserialize)]
pub struct LoginRequest {
    pub username: String,
    pub password: String,
}

/// What the frontend knows about the current user. `has_password` drives the
/// desktop "require login" toggle gate (an auto-provisioned account has none).
#[derive(Debug, Serialize)]
pub struct UserInfo {
    pub id: String,
    pub username: String,
    pub has_password: bool,
}

#[derive(Debug, Serialize)]
pub struct LoginResponse {
    pub token: String,
    pub user: UserInfo,
    pub expires_at: String,
}

#[derive(Debug, Deserialize)]
pub struct ChangePasswordRequest {
    pub current: String,
    pub new: String,
}

fn user_info(id: &str, username: &str, has_password: bool) -> UserInfo {
    UserInfo {
        id: id.to_string(),
        username: username.to_string(),
        has_password,
    }
}

fn session_expiry() -> String {
    (chrono::Utc::now() + Duration::days(SESSION_DAYS)).to_rfc3339()
}

/// `POST /api/auth/login` (public). Validate credentials, issue a session token.
pub async fn login(
    State(state): State<AppState>,
    Json(req): Json<LoginRequest>,
) -> Result<Json<LoginResponse>, StatusCode> {
    let user = state
        .storage
        .get_user_by_username(&req.username)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // ALWAYS run argon2 verification — against the real hash if the user exists,
    // or the cached dummy hash otherwise — so the 401 latency is identical and
    // usernames can't be enumerated by timing. Do not short-circuit on `None`.
    let password_ok = match &user {
        Some(u) => shirita_core::verify_password(&req.password, &u.password_hash),
        None => shirita_core::verify_password(&req.password, dummy_hash()),
    };

    let user = match (user, password_ok) {
        (Some(u), true) => u,
        _ => return Err(StatusCode::UNAUTHORIZED),
    };

    // Self-clean: drop expired sessions opportunistically on each login.
    let _ = state.storage.delete_expired_sessions().await;

    let token = shirita_core::random_token();
    let expires_at = session_expiry();
    state
        .storage
        .create_auth_session(&AuthSessionRecord::new(&token, &user.id, &expires_at))
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(LoginResponse {
        token,
        user: user_info(&user.id, &user.username, user.has_password()),
        expires_at,
    }))
}

/// `GET /api/auth/me` — the current session's user (drives the frontend's
/// logged-in state and the desktop "has password" toggle gate).
pub async fn me(
    State(state): State<AppState>,
    axum::Extension(AuthUser { id, .. }): axum::Extension<AuthUser>,
) -> Result<Json<UserInfo>, StatusCode> {
    let user = state
        .storage
        .get_user(&id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::UNAUTHORIZED)?;
    Ok(Json(user_info(&user.id, &user.username, user.has_password())))
}

/// `POST /api/auth/password` — change/set the password. If the account already
/// has a password, `current` must match; otherwise (desktop first-time set) it
/// is accepted without a current password. All other sessions are invalidated;
/// the current one survives.
pub async fn change_password(
    State(state): State<AppState>,
    axum::Extension(AuthUser { id, .. }): axum::Extension<AuthUser>,
    headers: HeaderMap,
    Json(req): Json<ChangePasswordRequest>,
) -> Result<StatusCode, StatusCode> {
    let user = state
        .storage
        .get_user(&id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::UNAUTHORIZED)?;

    if user.has_password() && !shirita_core::verify_password(&req.current, &user.password_hash) {
        return Err(StatusCode::FORBIDDEN);
    }
    if req.new.trim().is_empty() {
        return Err(StatusCode::BAD_REQUEST);
    }

    let new_hash = shirita_core::hash_password(&req.new)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    state
        .storage
        .update_user_password(&id, &new_hash)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // Invalidate every session except the current one.
    if let Some(current_token) = bearer_token(&headers) {
        let _ = state.storage.delete_sessions_for_user(&id, Some(current_token)).await;
    }
    Ok(StatusCode::OK)
}

/// `POST /api/auth/logout` — delete the current session token.
pub async fn logout(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> StatusCode {
    if let Some(token) = bearer_token(&headers) {
        let _ = state.storage.delete_auth_session(token).await;
    }
    StatusCode::OK
}
