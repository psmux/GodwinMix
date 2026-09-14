//! Who is calling, what they may do, and what they must confirm first.
//!
//! One bearer token with every scope is still the default and still works, so
//! nothing anybody has deployed stops. Beside it sits a `[tokens]` table so a
//! show can hand an agent a token that may take but may not remove, and a
//! rehearsal token that a live core refuses outright.

use crate::error::{ErrorCode, RpcError};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// What a token may reach. Ordered: `admin` implies `operate` implies `read`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Scope {
    Read,
    Operate,
    Admin,
}

impl Scope {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Read => "read",
            Self::Operate => "operate",
            Self::Admin => "admin",
        }
    }
}

/// Whether destructive calls need a confirm round trip first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum ConfirmPolicy {
    /// Destructive calls go straight through. The default, and what the single
    /// bearer token has always done.
    #[default]
    None,
    /// A destructive call is refused once with `-32020` and a token valid for
    /// 30 seconds; the repeat carrying `confirm` proceeds.
    Required,
}

/// Which MCP tool surface a token is meant for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Profile {
    /// At most 12 hot tools.
    #[default]
    Standard,
    /// Five tools, the rest behind `search_tools`, for a small context.
    Minimal,
}

impl Profile {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Standard => "standard",
            Self::Minimal => "minimal",
        }
    }
}

/// One credential, as the `[tokens]` table describes it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Token {
    /// Legible, and what `program.history` records against every take.
    pub id: String,
    pub secret: String,
    pub scopes: Vec<Scope>,
    pub confirm: ConfirmPolicy,
    /// Accepted only by a core started with `--rehearsal`.
    pub rehearsal: bool,
    pub profile: Profile,
}

impl Token {
    /// The single bearer token, which carries everything. This is the shape a
    /// deployment that has never heard of the table keeps getting.
    pub fn legacy(secret: &str) -> Self {
        Self {
            id: "default".into(),
            secret: secret.to_string(),
            scopes: vec![Scope::Read, Scope::Operate, Scope::Admin],
            confirm: ConfirmPolicy::None,
            rehearsal: false,
            profile: Profile::Standard,
        }
    }

    /// What an open control port grants: everything, under the id "open", so
    /// the history still says who took.
    pub fn open() -> Self {
        Self { id: "open".into(), secret: String::new(), ..Self::legacy("") }
    }

    pub fn has(&self, needed: Scope) -> bool {
        self.scopes.iter().any(|s| *s >= needed)
    }

    pub fn scope_names(&self) -> Vec<String> {
        self.scopes.iter().map(|s| s.as_str().to_string()).collect()
    }

    pub fn info(&self) -> crate::types::TokenInfo {
        crate::types::TokenInfo {
            id: self.id.clone(),
            scopes: self.scope_names(),
            confirm: match self.confirm {
                ConfirmPolicy::None => "none".into(),
                ConfirmPolicy::Required => "required".into(),
            },
            rehearsal: self.rehearsal,
            profile: self.profile.as_str().into(),
        }
    }
}

/// Every credential this core accepts, and whether it is a rehearsal core.
#[derive(Debug, Clone, Default)]
pub struct Tokens {
    entries: Vec<Token>,
    /// True when the core was started with `--rehearsal`.
    pub rehearsal_core: bool,
}

/// Why a presented secret was refused, in words the caller can act on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthFailure {
    Missing,
    Wrong,
    /// A rehearsal token against a live core, or a live token against a
    /// rehearsal core. Named separately because the fix is different.
    WrongCore(&'static str),
}

impl AuthFailure {
    pub fn message(&self) -> &'static str {
        match self {
            Self::Missing => "missing token",
            Self::Wrong => "wrong token",
            Self::WrongCore(m) => m,
        }
    }
}

impl Tokens {
    pub fn new(entries: Vec<Token>, rehearsal_core: bool) -> Self {
        Self { entries, rehearsal_core }
    }

    /// No token configured: the control port is open, which is how it has
    /// always worked and is fine behind a firewall.
    pub fn is_open(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn entries(&self) -> &[Token] {
        &self.entries
    }

    /// Match a presented secret. Compares against every entry without
    /// stopping early, so how long the check takes says nothing about which
    /// token was nearly right.
    pub fn authenticate(&self, presented: Option<&str>) -> Result<Token, AuthFailure> {
        if self.is_open() {
            return Ok(Token::open());
        }
        let Some(presented) = presented.filter(|p| !p.is_empty()) else {
            return Err(AuthFailure::Missing);
        };
        let mut found: Option<&Token> = None;
        for entry in &self.entries {
            if constant_time_eq(presented.as_bytes(), entry.secret.as_bytes()) {
                found = Some(entry);
            }
        }
        let Some(token) = found else { return Err(AuthFailure::Wrong) };
        // 09 section 5 item 14: an agent must not have to know which core it
        // is talking to, so the credential decides and the mismatch is refused
        // rather than quietly downgraded.
        match (token.rehearsal, self.rehearsal_core) {
            (true, false) => Err(AuthFailure::WrongCore(
                "this is a live core and that is a rehearsal token. \
                 Use the live token, or start the core with --rehearsal.",
            )),
            (false, true) => Err(AuthFailure::WrongCore(
                "this core was started with --rehearsal and that is a live token. \
                 Use a token with rehearsal = true.",
            )),
            _ => Ok(token.clone()),
        }
    }
}

/// Compare without stopping at the first difference, so how long the check
/// takes says nothing about how much of a guess was right. The length goes
/// into the same accumulator rather than being tested up front.
pub fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    let mut diff = a.len() ^ b.len();
    for i in 0..a.len().max(b.len()) {
        let x = a.get(i).copied().unwrap_or(0);
        let y = b.get(i).copied().unwrap_or(0);
        diff |= usize::from(x ^ y);
    }
    std::hint::black_box(diff) == 0
}

/// How long a confirm token stays good for. Long enough for a person to read
/// a dialog, short enough that one left lying about is no use.
pub const CONFIRM_TTL: Duration = Duration::from_secs(30);

/// The confirm tokens handed out by `-32020` refusals.
///
/// In memory and per process on purpose: a confirmation that survived a
/// restart would be a confirmation of a decision made about a different
/// programme.
#[derive(Debug, Default)]
pub struct Confirmations {
    issued: parking_lot::Mutex<HashMap<String, Pending>>,
    counter: AtomicU64,
}

#[derive(Debug, Clone)]
struct Pending {
    method: String,
    token_id: String,
    at: Instant,
}

impl Confirmations {
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    /// Refuse a destructive call and hand back the token that lets it through.
    pub fn require(&self, method: &str, token: &Token) -> RpcError {
        let id = self.counter.fetch_add(1, Ordering::Relaxed);
        let confirm_token = format!("cfm-{}-{id:x}", short_random());
        let mut issued = self.issued.lock();
        issued.retain(|_, p| p.at.elapsed() < CONFIRM_TTL);
        issued.insert(
            confirm_token.clone(),
            Pending { method: method.to_string(), token_id: token.id.clone(), at: Instant::now() },
        );
        RpcError::new(
            ErrorCode::ConfirmationRequired,
            format!(
                "{method} is destructive and token '{}' is set to confirm = required. \
                 Send the same call again with confirm = \"{confirm_token}\" within {} seconds.",
                token.id,
                CONFIRM_TTL.as_secs()
            ),
        )
        .with("confirm_token", confirm_token)
        .with("expires_in_ms", CONFIRM_TTL.as_millis() as u64)
        .with("method", method)
    }

    /// Spend a confirm token. It only works once, only for the method it was
    /// issued for, and only for the token that asked.
    pub fn redeem(&self, confirm: &str, method: &str, token: &Token) -> Result<(), RpcError> {
        let mut issued = self.issued.lock();
        issued.retain(|_, p| p.at.elapsed() < CONFIRM_TTL);
        match issued.remove(confirm) {
            Some(p) if p.method == method && p.token_id == token.id => Ok(()),
            Some(p) => {
                // Put it back: it was not this call's token, and spending it
                // here would break the call it was meant for.
                let wrong_method = p.method.clone();
                issued.insert(confirm.to_string(), p);
                Err(RpcError::new(
                    ErrorCode::ConfirmationRequired,
                    format!(
                        "that confirm token was issued for {wrong_method}, not for {method}. \
                         Call {method} once without confirm to get one for it."
                    ),
                ))
            }
            None => Err(RpcError::new(
                ErrorCode::ConfirmationRequired,
                format!(
                    "that confirm token is unknown or older than {} seconds. \
                     Call {method} once without confirm to get a fresh one.",
                    CONFIRM_TTL.as_secs()
                ),
            )),
        }
    }
}

/// A few hex digits with no crate behind them: the process clock, which is
/// enough to keep two tokens issued in the same millisecond apart once the
/// counter is on the end of it.
fn short_random() -> String {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.subsec_nanos() as u64 ^ d.as_secs())
        .unwrap_or(0);
    format!("{nanos:08x}")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn token(id: &str, secret: &str, scopes: &[Scope]) -> Token {
        Token {
            id: id.into(),
            secret: secret.into(),
            scopes: scopes.to_vec(),
            confirm: ConfirmPolicy::None,
            rehearsal: false,
            profile: Profile::Standard,
        }
    }

    #[test]
    fn no_tokens_configured_leaves_the_port_open_with_every_scope() {
        let t = Tokens::default();
        assert!(t.is_open());
        let who = t.authenticate(None).unwrap();
        assert!(who.has(Scope::Admin));
        assert_eq!(who.id, "open");
    }

    /// The single bearer token keeps working and keeps every scope, which is
    /// the promise to everyone who has already deployed one.
    #[test]
    fn the_single_bearer_token_still_carries_everything() {
        let t = Tokens::new(vec![Token::legacy("s3cret")], false);
        let who = t.authenticate(Some("s3cret")).unwrap();
        assert!(who.has(Scope::Read) && who.has(Scope::Operate) && who.has(Scope::Admin));
        assert_eq!(who.confirm, ConfirmPolicy::None);
        assert_eq!(t.authenticate(Some("nope")), Err(AuthFailure::Wrong));
        assert_eq!(t.authenticate(None), Err(AuthFailure::Missing));
    }

    #[test]
    fn scopes_are_a_ladder_not_a_set() {
        let reader = token("bot", "a", &[Scope::Read]);
        assert!(reader.has(Scope::Read));
        assert!(!reader.has(Scope::Operate));
        let operator = token("desk", "b", &[Scope::Operate]);
        assert!(operator.has(Scope::Read), "operate can read");
        assert!(!operator.has(Scope::Admin));
        let admin = token("me", "c", &[Scope::Admin]);
        assert!(admin.has(Scope::Read) && admin.has(Scope::Operate) && admin.has(Scope::Admin));
    }

    /// A credential decides which core it belongs to, so an agent that cannot
    /// tell rehearsal from live does not have to.
    #[test]
    fn a_rehearsal_token_and_a_live_core_refuse_each_other() {
        let rehearsal = Token { rehearsal: true, ..token("bot", "r", &[Scope::Operate]) };
        let live = token("desk", "l", &[Scope::Operate]);

        let live_core = Tokens::new(vec![rehearsal.clone(), live.clone()], false);
        assert!(live_core.authenticate(Some("l")).is_ok());
        assert!(matches!(live_core.authenticate(Some("r")), Err(AuthFailure::WrongCore(_))));

        let rehearsal_core = Tokens::new(vec![rehearsal, live], true);
        assert!(rehearsal_core.authenticate(Some("r")).is_ok());
        assert!(matches!(rehearsal_core.authenticate(Some("l")), Err(AuthFailure::WrongCore(_))));
    }

    #[test]
    fn a_confirm_token_works_once_for_one_method() {
        let c = Confirmations::default();
        let who = Token { confirm: ConfirmPolicy::Required, ..token("bot", "x", &[Scope::Operate]) };
        let refusal = c.require("source.remove", &who);
        assert_eq!(refusal.code, ErrorCode::ConfirmationRequired.number());
        let ct = refusal.data["confirm_token"].as_str().unwrap().to_string();
        assert!(refusal.message.contains(&ct), "the message has to carry the token: {refusal}");

        // Wrong method: refused, and the token survives for the call it was
        // issued for.
        assert!(c.redeem(&ct, "output.remove", &who).is_err());
        assert!(c.redeem(&ct, "source.remove", &who).is_ok());
        // Spent.
        assert!(c.redeem(&ct, "source.remove", &who).is_err());
        // Never issued.
        assert!(c.redeem("cfm-nope", "source.remove", &who).is_err());
    }

    /// Another token's confirmation is no use, or a read only agent could
    /// borrow the operator's dialog.
    #[test]
    fn a_confirm_token_belongs_to_the_token_that_asked() {
        let c = Confirmations::default();
        let mine = Token { confirm: ConfirmPolicy::Required, ..token("a", "1", &[Scope::Operate]) };
        let yours = Token { confirm: ConfirmPolicy::Required, ..token("b", "2", &[Scope::Operate]) };
        let ct = c.require("source.remove", &mine).data["confirm_token"].as_str().unwrap().to_string();
        assert!(c.redeem(&ct, "source.remove", &yours).is_err());
        assert!(c.redeem(&ct, "source.remove", &mine).is_ok());
    }

    #[test]
    fn constant_time_eq_agrees_with_plain_equality() {
        assert!(constant_time_eq(b"", b""));
        assert!(constant_time_eq(b"abc", b"abc"));
        assert!(!constant_time_eq(b"abc", b"abd"));
        assert!(!constant_time_eq(b"abc", b"ab"));
        assert!(!constant_time_eq(b"ab", b"abc"));
        assert!(!constant_time_eq(b"abc", b""));
    }
}
