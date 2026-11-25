//! Passwords: argon2id with the crate's default cost, checked in constant
//! time, and a check that costs the same whether or not the account exists.

use argon2::password_hash::phc::PasswordHash;
use argon2::password_hash::{PasswordHasher, PasswordVerifier};
use argon2::Argon2;
use godwinmix_protocol::error::RpcError;
use std::sync::OnceLock;

/// Shortest password accepted. Long enough that guessing it through the rate
/// limit takes years; short enough to type on a phone.
pub const MIN_LEN: usize = 8;
/// Longest, so a megabyte of password is not hashed on request.
pub const MAX_LEN: usize = 1024;

/// Why a new password is refused, if it is.
pub fn check_new(password: &str) -> Result<(), RpcError> {
    let n = password.chars().count();
    if n < MIN_LEN {
        return Err(RpcError::invalid_params(format!(
            "that password has {n} characters and needs at least {MIN_LEN}. Choose a longer one; a few words together are easy to remember."
        ))
        .with("field", "password")
        .with("min_length", MIN_LEN as u64));
    }
    if password.len() > MAX_LEN {
        return Err(RpcError::invalid_params(format!("that password is longer than {MAX_LEN} bytes. Choose a shorter one."))
            .with("field", "password"));
    }
    if password.trim() != password {
        return Err(RpcError::invalid_params(
            "that password starts or ends with a space, which is easy to lose when it is typed again. Take the space out.",
        )
        .with("field", "password"));
    }
    Ok(())
}

/// The PHC string for a new password. Slow on purpose: call it off the
/// thread that reads the socket.
pub fn hash(password: &str) -> Result<String, RpcError> {
    Argon2::default()
        .hash_password(password.as_bytes())
        .map(|h| h.to_string())
        .map_err(|e| RpcError::internal(format!("the password could not be hashed: {e}. Nothing changed; try again.")))
}

/// Whether `password` matches `stored`. A stored string that does not parse
/// matches nothing.
pub fn verify(password: &str, stored: &str) -> bool {
    let Ok(parsed) = PasswordHash::new(stored) else { return false };
    Argon2::default().verify_password(password.as_bytes(), &parsed).is_ok()
}

/// The same work as [`verify`], against a hash nobody knows the password to.
/// Run when the username is unknown or disabled, so how long a refusal takes
/// says nothing about which it was.
pub fn verify_nothing(password: &str) {
    let _ = verify(password, dummy());
}

/// Make the dummy hash now, before any timing that matters, so the first
/// unknown name is not the one slow refusal.
pub fn warm() {
    let _ = dummy();
}

fn dummy() -> &'static str {
    static DUMMY: OnceLock<String> = OnceLock::new();
    DUMMY.get_or_init(|| {
        let random = godwinmix_core::secrets::random_key(32).unwrap_or_else(|_| "no-such-account-xxxxxxxx".into());
        hash(&random).unwrap_or_default()
    })
}
