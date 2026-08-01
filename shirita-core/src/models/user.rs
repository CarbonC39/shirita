//! Auth: a user account (username + argon2 password hash).

use serde::{Deserialize, Serialize};

/// A row in the `users` table. `password_hash` is an argon2 PHC string; an empty
/// string means "no interactive password set yet" — a desktop auto-provisioned
/// account until the user sets a password in Settings (and until then it cannot
/// be used to log in interactively).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct User {
    pub id: String,
    pub username: String,
    pub password_hash: String,
    pub created_at: String,
}

impl User {
    pub fn new(username: impl Into<String>, password_hash: impl Into<String>) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            username: username.into(),
            password_hash: password_hash.into(),
            created_at: chrono::Utc::now().to_rfc3339(),
        }
    }

    /// Whether an interactive password has been set (non-empty hash). `false` for
    /// a desktop auto-provisioned account until the user sets a password.
    pub fn has_password(&self) -> bool {
        !self.password_hash.is_empty()
    }
}
