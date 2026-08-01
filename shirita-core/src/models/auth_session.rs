//! Auth: an opaque session token row.

use serde::{Deserialize, Serialize};

/// A row in the `sessions` table — an opaque bearer token bound to a user with
/// an absolute expiry. Named `AuthSessionRecord` (not `Session`) to avoid
/// clashing with the domain chat model `models::session::Session`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AuthSessionRecord {
    /// The bearer secret (32 random bytes, base64url). Primary key.
    pub token: String,
    pub user_id: String,
    /// RFC3339 timestamp the session was issued.
    pub created_at: String,
    /// RFC3339 timestamp the session stops being valid.
    pub expires_at: String,
}

impl AuthSessionRecord {
    /// New session record with the given token/user and an absolute expiry
    /// timestamp. The caller generates the token and computes `expires_at`.
    pub fn new(token: impl Into<String>, user_id: impl Into<String>, expires_at: impl Into<String>) -> Self {
        Self {
            token: token.into(),
            user_id: user_id.into(),
            created_at: chrono::Utc::now().to_rfc3339(),
            expires_at: expires_at.into(),
        }
    }
}
