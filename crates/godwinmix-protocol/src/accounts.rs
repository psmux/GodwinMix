//! Accounts: a username and a password for the people who run a show.
//!
//! Optional. A mixer with no account works exactly as it always has, with its
//! bearer tokens. Once one account exists the page asks for a username and a
//! password instead of a token, and a sign in makes a session. The session is
//! a token underneath: it carries the account's scopes, its confirm policy and
//! its safety numbers, and it authenticates through `Tokens` like any other,
//! so every scope check, rate limit and history line keeps working unchanged.
//!
//! The types and the seam are here. The store, the password hashing and the
//! cookie are in the binary, because this crate has no file system and no
//! hash function.

use crate::scope::{ConfirmPolicy, Scope, Token, TokenSafety};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// What an account may do. The same ladder as a token's scopes, in the words
/// a person picking one for a colleague would use.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema, Default)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    /// Watches: the multiview, the programme, the state. Takes nothing.
    View,
    /// Runs the show: takes, transitions, graphics, sources and outputs.
    #[default]
    Operate,
    /// Everything, including Mixer settings and the users themselves.
    Admin,
}

impl Role {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::View => "view",
            Self::Operate => "operate",
            Self::Admin => "admin",
        }
    }

    /// The scope a session of this role carries. The ladder implies the rest.
    pub const fn scope(self) -> Scope {
        match self {
            Self::View => Scope::Read,
            Self::Operate => Scope::Operate,
            Self::Admin => Scope::Admin,
        }
    }

    pub fn parse(text: &str) -> Option<Self> {
        match text.trim().to_ascii_lowercase().as_str() {
            "view" | "viewer" | "read" => Some(Self::View),
            "operate" | "operator" => Some(Self::Operate),
            "admin" => Some(Self::Admin),
            _ => None,
        }
    }
}

/// One account, without its password.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct UserInfo {
    /// The username. Also the id every take is recorded against in
    /// `program.history`.
    pub name: String,
    pub role: Role,
    /// A disabled account cannot sign in, and its sessions are ended.
    #[serde(default)]
    pub disabled: bool,
    /// `required` asks for a confirmation before a destructive call.
    #[serde(default)]
    pub confirm: ConfirmPolicy,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub safety: Option<TokenSafety>,
    /// When it was made, RFC 3339 in UTC.
    pub created: String,
    /// How many sessions are signed in now.
    #[serde(default)]
    pub sessions: usize,
}

/// One signed in browser or device.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct SessionInfo {
    /// A short id for this session, to end it with `user.sign_out`. Never the
    /// secret.
    pub id: String,
    pub user: String,
    /// When it signed in, RFC 3339 in UTC.
    pub created: String,
    /// When it stops working unless the person signs in again.
    pub expires: String,
    /// The address it signed in from, as the mixer saw it.
    #[serde(default)]
    pub address: String,
    /// The browser's own description of itself, shortened.
    #[serde(default)]
    pub device: String,
    /// When this mixer last saw it, since the mixer started. Absent when it
    /// has not been seen since then.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_seen: Option<String>,
    /// True for the session that made this call.
    #[serde(default)]
    pub current: bool,
}

/// `user.list`.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct UserList {
    pub users: Vec<UserInfo>,
}

/// `user.add`.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct UserAddRequest {
    /// Lower case letters, digits, `.`, `-` and `_`, up to 32 characters.
    pub name: String,
    /// `view`, `operate` (the default) or `admin`.
    #[serde(default)]
    pub role: Role,
    /// At least 8 characters.
    pub password: String,
    #[serde(default)]
    pub confirm: ConfirmPolicy,
}

/// `user.update`: whatever is given changes, the rest stays.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct UserUpdateRequest {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<Role>,
    /// True ends every session of the account at once and refuses its sign in.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub disabled: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub confirm: Option<ConfirmPolicy>,
}

/// `user.reset_password`.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct UserPasswordRequest {
    pub name: String,
    /// The new password, at least 8 characters.
    pub password: String,
}

/// `user.remove` and `user.sessions`.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct UserNameRequest {
    pub name: String,
}

/// `user.sessions` with no name lists everybody's.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
pub struct UserSessionsRequest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

/// `user.sign_out`: one session by its id, or every session of one user.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
pub struct UserSignOutRequest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session: Option<String>,
}

/// What `user.add`, `user.update`, `user.reset_password` and `user.remove`
/// answer with.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct UserChanged {
    pub user: UserInfo,
    /// How many sessions the change ended.
    #[serde(default)]
    pub sessions_ended: usize,
}

/// `user.sessions`.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct SessionList {
    pub sessions: Vec<SessionInfo>,
}

/// `user.sign_out`.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct SignedOut {
    pub sessions_ended: usize,
}

/// `account.get`: who this call is, in a person's terms.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct AccountInfo {
    /// True when the mixer has at least one account, so the page signs people
    /// in with a username rather than a token.
    pub accounts: bool,
    /// The account this call signed in as. Absent for a token.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<Role>,
    /// `session` for a sign in, `token` for a bearer token, `open` for a
    /// mixer that asks for nothing.
    pub via: String,
}

/// `account.change_password`.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct AccountPasswordRequest {
    pub current: String,
    /// At least 8 characters.
    pub password: String,
}

/// Where accounts and sessions are kept and checked. `Tokens` asks it about
/// any secret the configured, minted and device lists do not know.
///
/// Every method is quick and touches at most one small file.
pub trait AccountRegistry: Send + Sync + std::fmt::Debug {
    /// The token a presented session secret stands for, when it is a live
    /// session of an enabled account.
    fn find(&self, presented: &str) -> Option<Token>;
    /// True when `token` was a session from here and has since been ended,
    /// or its account was disabled or removed.
    fn revoked(&self, token: &Token) -> bool;
    /// True when at least one account exists. Such a mixer is never open.
    fn any(&self) -> bool;
    /// The concrete store, for the binary's management calls.
    fn as_any(&self) -> &dyn std::any::Any;
}
