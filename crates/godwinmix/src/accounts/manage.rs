//! Adding, changing and removing accounts, and ending their sessions. The
//! same code answers the Users card, the agent tools and `godwinmix users`.
//!
//! Every function here is synchronous and some hash a password, which takes
//! a few tens of milliseconds: call them on the blocking pool.

use godwinmix_protocol::accounts::{Role, SessionInfo, UserAddRequest, UserChanged, UserInfo, UserUpdateRequest};
use godwinmix_protocol::error::{ErrorCode, RpcError};

use super::store::{self, User};
use super::{password, Accounts, State};

/// Longest username kept.
pub const NAME_MAX: usize = 32;

/// A username as it is kept: trimmed and lower case. Refused when it is
/// empty, too long, or has anything but letters, digits, `.`, `-` and `_`.
pub fn check_name(name: &str) -> Result<String, RpcError> {
    let name = name.trim().to_lowercase();
    let fine = !name.is_empty()
        && name.len() <= NAME_MAX
        && name.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '.' | '-' | '_'))
        && name.chars().next().is_some_and(|c| c.is_ascii_alphanumeric());
    if fine {
        return Ok(name);
    }
    Err(RpcError::invalid_params(format!(
        "'{name}' cannot be a username. Use up to {NAME_MAX} lower case letters, digits, dots, dashes or underscores, starting with a letter or a digit: sam, cam-op-2."
    ))
    .with("field", "name"))
}

fn not_found(state: &State, name: &str) -> RpcError {
    let known: Vec<String> = state.file.users.iter().map(|u| u.name.clone()).collect();
    RpcError::not_found("user", name, &known)
}

fn count(state: &State, name: &str) -> usize {
    state.file.sessions.iter().filter(|s| s.user == name).count()
}

/// End every session of `name`, and say how many there were.
fn end_sessions(state: &mut State, name: &str) -> usize {
    let before = state.file.sessions.len();
    state.file.sessions.retain(|s| s.user != name);
    before - state.file.sessions.len()
}

/// True when `name` is the only enabled admin, so taking it away would leave
/// nobody able to manage the users from a page.
fn last_admin(state: &State, name: &str) -> bool {
    let admins: Vec<&User> = state.file.users.iter().filter(|u| u.role == Role::Admin && !u.disabled).collect();
    admins.len() == 1 && admins[0].name == name
}

fn refuse_last_admin(what: &str, name: &str) -> RpcError {
    RpcError::new(
        ErrorCode::NotInState,
        format!("{name} is the only admin account left, so it cannot be {what}. Make another account an admin first."),
    )
    .with("name", name)
}

impl Accounts {
    /// The role an account holds now.
    pub fn role_of(&self, name: &str) -> Option<Role> {
        let mut state = self.state.lock();
        self.refresh(&mut state, false);
        state.file.users.iter().find(|u| u.name == name).map(|u| u.role)
    }

    pub fn list(&self) -> Vec<UserInfo> {
        let mut state = self.state.lock();
        self.refresh(&mut state, true);
        state.file.users.iter().map(|u| u.public(count(&state, &u.name))).collect()
    }

    /// Make an account. `taken` says whether a name is already some token's
    /// id, so a history line never means two people.
    pub fn add(&self, req: UserAddRequest, taken: &dyn Fn(&str) -> bool) -> Result<UserChanged, RpcError> {
        let name = check_name(&req.name)?;
        password::check_new(&req.password)?;
        if taken(&name) {
            return Err(RpcError::invalid_params(format!(
                "'{name}' is already the id of a token on this mixer. Choose another username."
            ))
            .with("field", "name"));
        }
        let hash = password::hash(&req.password)?;
        let mut state = self.state.lock();
        self.refresh(&mut state, true);
        if state.file.users.iter().any(|u| u.name == name) {
            return Err(RpcError::invalid_params(format!(
                "there is already an account named '{name}'. Choose another username, or reset that account's password instead."
            ))
            .with("field", "name"));
        }
        let user = User { name, role: req.role, disabled: false, confirm: req.confirm, safety: None, created: crate::channels::keys::now(), password: hash };
        state.file.users.push(user.clone());
        if let Err(e) = self.persist(&mut state) {
            state.file.users.pop();
            return Err(e);
        }
        tracing::info!(user = %user.name, role = user.role.as_str(), "account added");
        Ok(UserChanged { user: user.public(0), sessions_ended: 0 })
    }

    pub fn update(&self, req: UserUpdateRequest) -> Result<UserChanged, RpcError> {
        let name = req.name.trim().to_lowercase();
        let mut state = self.state.lock();
        self.refresh(&mut state, true);
        let Some(at) = state.file.users.iter().position(|u| u.name == name) else { return Err(not_found(&state, &name)) };
        let demoted = req.role.is_some_and(|r| r != Role::Admin);
        if (demoted || req.disabled == Some(true)) && last_admin(&state, &name) {
            return Err(refuse_last_admin(if demoted { "given a lower role" } else { "disabled" }, &name));
        }
        let before = state.file.clone_users();
        let user = &mut state.file.users[at];
        if let Some(role) = req.role { user.role = role; }
        if let Some(disabled) = req.disabled { user.disabled = disabled; }
        if let Some(confirm) = req.confirm { user.confirm = confirm; }
        let disabled = user.disabled;
        let ended = if disabled { end_sessions(&mut state, &name) } else { 0 };
        if let Err(e) = self.persist(&mut state) {
            state.file.users = before;
            return Err(e);
        }
        tracing::info!(user = %name, disabled, ended, "account changed");
        let user = &state.file.users[at];
        Ok(UserChanged { user: user.public(count(&state, &name)), sessions_ended: ended })
    }

    /// Give an account a new password and end every session it has: whoever
    /// knew the old one is signed out wherever they are.
    pub fn reset_password(&self, name: &str, new: &str, keep: Option<&str>) -> Result<UserChanged, RpcError> {
        let name = name.trim().to_lowercase();
        password::check_new(new)?;
        let hash = password::hash(new)?;
        let mut state = self.state.lock();
        self.refresh(&mut state, true);
        let Some(at) = state.file.users.iter().position(|u| u.name == name) else { return Err(not_found(&state, &name)) };
        state.file.users[at].password = hash;
        let before = state.file.sessions.len();
        state.file.sessions.retain(|s| s.user != name || keep.is_some_and(|k| k == s.sha256));
        let ended = before - state.file.sessions.len();
        self.persist(&mut state)?;
        tracing::info!(user = %name, ended, "password changed");
        Ok(UserChanged { user: state.file.users[at].public(count(&state, &name)), sessions_ended: ended })
    }

    pub fn remove(&self, name: &str) -> Result<UserChanged, RpcError> {
        let name = name.trim().to_lowercase();
        let mut state = self.state.lock();
        self.refresh(&mut state, true);
        let Some(at) = state.file.users.iter().position(|u| u.name == name) else { return Err(not_found(&state, &name)) };
        if last_admin(&state, &name) && state.file.users.len() > 1 {
            return Err(refuse_last_admin("removed while other accounts remain", &name));
        }
        let gone = state.file.users.remove(at);
        let ended = end_sessions(&mut state, &name);
        self.persist(&mut state)?;
        tracing::info!(user = %name, ended, "account removed");
        Ok(UserChanged { user: gone.public(0), sessions_ended: ended })
    }

    /// Every session, or one user's, newest first. `current` marks the
    /// session whose digest is given.
    pub fn sessions(&self, name: Option<&str>, current: Option<&str>) -> Vec<SessionInfo> {
        let mut state = self.state.lock();
        self.refresh(&mut state, true);
        let now = store::now_secs();
        let mut out: Vec<SessionInfo> = state
            .file
            .sessions
            .iter()
            .filter(|s| s.expires > now && name.is_none_or(|n| s.user == n.trim().to_lowercase()))
            .map(|s| SessionInfo {
                last_seen: state.seen_at.get(&s.sha256).map(godwinmix_core::observe::logs::rfc3339),
                current: current.is_some_and(|c| c == s.sha256),
                ..s.public()
            })
            .collect();
        out.sort_by(|a, b| b.created.cmp(&a.created));
        out
    }

    /// End one session by its id, or every session of one user, or (with
    /// neither) every session of everybody.
    pub fn sign_out(&self, name: Option<&str>, session: Option<&str>) -> Result<usize, RpcError> {
        let mut state = self.state.lock();
        self.refresh(&mut state, true);
        let name = name.map(|n| n.trim().to_lowercase());
        if let Some(n) = &name {
            if !state.file.users.iter().any(|u| &u.name == n) {
                return Err(not_found(&state, n));
            }
        }
        let before = state.file.sessions.len();
        state.file.sessions.retain(|s| {
            let named = name.as_ref().is_none_or(|n| &s.user == n);
            let picked = session.is_none_or(|id| s.id == id);
            !(named && picked)
        });
        let ended = before - state.file.sessions.len();
        if session.is_some() && ended == 0 {
            return Err(RpcError::not_found("session", session.unwrap_or_default(), &[]));
        }
        self.persist(&mut state)?;
        tracing::info!(user = ?name, ended, "sessions ended");
        Ok(ended)
    }
}

impl store::File {
    fn clone_users(&self) -> Vec<User> {
        self.users.clone()
    }
}
