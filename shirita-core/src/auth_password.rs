//! Password hashing (argon2id).
//!
//! Shared by the bootstrap seeder (`seed::ensure_bootstrap_user`) and the login
//! handler (`shirita_web::routes::auth`). Both call [`hash_password`], so real
//! and dummy hashes are produced with identical `Params` and the same salt
//! generation path — this is what makes the login dummy-hash mitigation
//! timing-safe: `verify_password` runs the full KDF for the same cost on both
//! the unknown-user and wrong-password paths.

use argon2::password_hash::{
    rand_core::OsRng, Error as PhError, PasswordHash, PasswordHasher, PasswordVerifier, SaltString,
};
use argon2::{Algorithm, Argon2, Params, Version};
use crate::{Error, Result};

/// argon2id cost: 19456 KiB (~19 MiB) memory, 2 iterations, parallelism 1.
/// Noticeable but affordable on the single-threaded login path.
fn params() -> Result<Params> {
    Params::new(19_456, 2, 1, None).map_err(|e| Error::Config(format!("argon2 params: {e}")))
}

fn hasher() -> Result<Argon2<'static>> {
    Ok(Argon2::new(Algorithm::Argon2id, Version::V0x13, params()?))
}

/// Hash a plaintext password, returning the argon2 PHC string. Every call uses
/// the same `Params` and a fresh random salt — real passwords and the login
/// dummy hash alike.
pub fn hash_password(plain: &str) -> Result<String> {
    let mut rng = OsRng;
    let salt = SaltString::generate(&mut rng);
    let hash = hasher()?
        .hash_password(plain.as_bytes(), &salt)
        .map_err(ph_err)?;
    Ok(hash.to_string())
}

/// Verify a plaintext password against a PHC string. Returns `false` for any
/// mismatch or unparseable hash — it never propagates an error, so a bad stored
/// hash simply means "login denied."
pub fn verify_password(plain: &str, phc: &str) -> bool {
    let Ok(parsed) = PasswordHash::new(phc) else {
        return false;
    };
    hasher()
        .map(|h| h.verify_password(plain.as_bytes(), &parsed).is_ok())
        .unwrap_or(false)
}

/// 32 cryptographically-random bytes → base64url (no padding). The opaque
/// bearer session-token secret issued at login / desktop auto-provision.
pub fn random_token() -> String {
    use base64::Engine;
    let mut bytes = [0u8; 32];
    rand::RngCore::fill_bytes(&mut rand::thread_rng(), &mut bytes);
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

/// RFC3339 timestamp `days` from now — the session-expiry convention shared by
/// the login handler (30d) and the desktop auto-session (365d).
pub fn iso_now_plus_days(days: i64) -> String {
    (chrono::Utc::now() + chrono::Duration::days(days)).to_rfc3339()
}

fn ph_err(e: PhError) -> Error {
    Error::Config(format!("password hash: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_round_trips() {
        let phc = hash_password("correct horse").unwrap();
        assert!(verify_password("correct horse", &phc));
        assert!(!verify_password("battery staple", &phc));
    }

    #[test]
    fn each_hash_has_a_unique_salt() {
        let a = hash_password("same").unwrap();
        let b = hash_password("same").unwrap();
        assert_ne!(a, b, "salt must differ per hash");
        assert!(verify_password("same", &a) && verify_password("same", &b));
    }

    #[test]
    fn verify_rejects_unparseable_hash() {
        assert!(!verify_password("x", "not-a-real-phc"));
        assert!(!verify_password("x", ""));
    }
}
