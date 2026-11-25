//! The half of the store that `Tokens` calls on every request.

use godwinmix_protocol::accounts::AccountRegistry;
use godwinmix_protocol::scope::{constant_time_eq, Token};

use super::{store, token_for, Accounts, State, SECRET_PREFIX};

impl Accounts {
    /// The token for a session digest, when the session is live and its
    /// account exists and is enabled.
    pub(crate) fn lookup(&self, state: &State, digest: &str) -> Option<Token> {
        let now = store::now_secs();
        let session = state
            .file
            .sessions
            .iter()
            .find(|s| constant_time_eq(s.sha256.as_bytes(), digest.as_bytes()) && s.expires > now)?;
        let user = state.file.users.iter().find(|u| u.name == session.user && !u.disabled)?;
        Some(token_for(user, session))
    }
}

impl AccountRegistry for Accounts {
    fn find(&self, presented: &str) -> Option<Token> {
        let digest = store::digest(presented);
        let mut state = self.state.lock();
        self.refresh(&mut state, false);
        let found = self.lookup(&state, &digest).or_else(|| {
            // Unknown here, but another process may have just signed it in.
            self.refresh(&mut state, true);
            self.lookup(&state, &digest)
        });
        if found.is_some() {
            state.seen_at.insert(digest, std::time::SystemTime::now());
        }
        found
    }

    /// Ended, or its account disabled, removed or given another role. A
    /// connection that signed in as an admin must not stay one after the
    /// account was made a viewer; refused, the page connects again and gets
    /// the new role. A session that merely ran out is not revoked: a
    /// connection opened before it ran out stays open.
    fn revoked(&self, token: &Token) -> bool {
        let Some(digest) = token.secret.strip_prefix(SECRET_PREFIX) else { return false };
        let mut state = self.state.lock();
        self.refresh(&mut state, false);
        let Some(session) = state.file.sessions.iter().find(|s| s.sha256 == digest) else { return true };
        let Some(user) = state.file.users.iter().find(|u| u.name == session.user && !u.disabled) else { return true };
        token.scopes != [user.role.scope()] || token.confirm != user.confirm
    }

    fn any(&self) -> bool {
        let mut state = self.state.lock();
        self.refresh(&mut state, false);
        !state.file.users.is_empty()
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}
