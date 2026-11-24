//! Signing in and out, and a person changing their own password.

use std::time::Duration;

use godwinmix_protocol::accounts::Role;
use godwinmix_protocol::error::RpcError;
use godwinmix_protocol::scope::Token;

use super::store::{self, Session};
use super::{password, Accounts, SECRET_PREFIX, SESSION_DAYS};

/// Why a sign in was refused. The words never say which of the username and
/// the password was wrong, or whether the account exists.
#[derive(Debug, PartialEq, Eq)]
pub enum Refused {
    /// The username and the password do not match an enabled account.
    NoMatch,
    /// Too many wrong tries; try again after this long.
    Wait(Duration),
    /// The store could not be written; the message says why.
    Store(String),
}

impl Refused {
    pub fn message(&self) -> String {
        match self {
            Self::NoMatch => "That username and password do not match an account on this mixer. Check both and try again; an admin can reset a forgotten password.".into(),
            Self::Wait(d) => format!(
                "Too many wrong tries. Wait {} minutes and try again, or ask an admin to reset the password.",
                d.as_secs().div_ceil(60).max(1)
            ),
            Self::Store(why) => why.clone(),
        }
    }
}

/// A new session: the secret for the cookie, the account it belongs to.
pub struct SignedIn {
    pub secret: String,
    pub user: String,
    pub role: Role,
}

impl Accounts {
    /// Check a username and password and make a session. Takes as long for
    /// an unknown name as for a known one. Call it on the blocking pool.
    pub fn sign_in(&self, name: &str, given: &str, address: &str, device: &str) -> Result<SignedIn, Refused> {
        let name = name.trim().to_lowercase();
        password::warm();
        if let Some(wait) = self.limiter.check(address, &name) {
            return Err(Refused::Wait(wait));
        }
        let stored = {
            let mut state = self.state.lock();
            self.refresh(&mut state, true);
            state.file.users.iter().find(|u| u.name == name && !u.disabled).map(|u| (u.password.clone(), u.role))
        };
        // One hash checked either way, so an unknown name costs as long.
        let matched = match &stored {
            Some((hash, _)) => password::verify(given, hash),
            None => {
                password::verify_nothing(given);
                false
            }
        };
        let Some((_, role)) = stored.filter(|_| matched) else {
            self.limiter.failed(address, &name);
            tracing::info!(%address, "a sign in was refused");
            return Err(Refused::NoMatch);
        };
        self.limiter.succeeded(&name);
        let fresh = |n| godwinmix_core::secrets::random_key(n).map_err(|e| Refused::Store(format!("{e:#}. Try again.")));
        let secret = fresh(40)?;
        let id = fresh(10)?;
        let session = Session {
            id: format!("s-{id}"),
            user: name.clone(),
            sha256: store::digest(&secret),
            created: crate::channels::keys::now(),
            expires: store::now_secs() + SESSION_DAYS * 86_400,
            address: address.chars().take(64).collect(),
            device: device.chars().take(120).collect(),
        };
        let mut state = self.state.lock();
        self.refresh(&mut state, true);
        state.file.sessions.push(session);
        if let Err(e) = self.persist(&mut state) {
            state.file.sessions.pop();
            return Err(Refused::Store(e.message));
        }
        tracing::info!(user = %name, %address, "signed in");
        Ok(SignedIn { secret, user: name, role })
    }

    /// End the session behind a secret. False when there was none.
    pub fn sign_out_secret(&self, secret: &str) -> bool {
        let digest = store::digest(secret);
        let mut state = self.state.lock();
        self.refresh(&mut state, true);
        let before = state.file.sessions.len();
        state.file.sessions.retain(|s| s.sha256 != digest);
        let ended = before != state.file.sessions.len();
        if ended && self.persist(&mut state).is_err() {
            tracing::warn!("a sign out could not be written; the session ends when the file can be");
        }
        ended
    }

    /// A person changing their own password: the current one first. Every
    /// other session of theirs ends; this one stays signed in.
    pub fn change_own(&self, token: &Token, current: &str, new: &str) -> Result<usize, RpcError> {
        let Some(digest) = token.secret.strip_prefix(SECRET_PREFIX) else {
            return Err(RpcError::not_in_state(
                "this call came with a token, not a sign in, so there is no password of its own to change. An admin changes an account's password with user.reset_password.",
            ));
        };
        let hash = {
            let mut state = self.state.lock();
            self.refresh(&mut state, true);
            state.file.users.iter().find(|u| u.name == token.id).map(|u| u.password.clone())
        };
        if !hash.is_some_and(|h| password::verify(current, &h)) {
            return Err(RpcError::invalid_params("that is not your current password. Nothing changed.").with("field", "current"));
        }
        Ok(self.reset_password(&token.id, new, Some(digest))?.sessions_ended)
    }
}

