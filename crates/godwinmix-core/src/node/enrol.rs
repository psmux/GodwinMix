//! Enrolment tokens: the one secret that ever crosses in the clear.
//!
//! An operator runs `gmx node token --name studio-b` on the core, carries the
//! string to the other machine, and runs `godwinmix node --core ... --token
//! ...`. The node sends it once, gets a certificate back, and from then on
//! every byte is mutual TLS. The token is good for one use and expires whether
//! it is used or not.
//!
//! Redeemed tokens are kept rather than deleted, because "that token was
//! already used at 19:42 by studio-b" is a better answer than "no such token",
//! and because a second machine replaying a token an operator pasted into a
//! chat window is exactly the thing worth naming in a log.

use anyhow::{Context, Result};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// How long a minted token lives if nobody says otherwise. Long enough to walk
/// to the other machine, short enough that one left in a terminal history is
/// not a key to the building.
pub const DEFAULT_TTL: Duration = Duration::from_secs(60 * 60);

/// One minted enrolment token.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Ticket {
    /// The secret itself. Present on the record the minter returns and on
    /// disk; `node.list` never shows it.
    pub token: String,
    /// The node this token may enrol as. A token minted for `studio-b` does
    /// not enrol `studio-c`.
    pub name: String,
    pub minted_unix: u64,
    pub expires_unix: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub used_unix: Option<u64>,
}

impl Ticket {
    pub fn expired(&self, now: u64) -> bool {
        now >= self.expires_unix
    }

    pub fn used(&self) -> bool {
        self.used_unix.is_some()
    }
}

/// Why an enrolment was refused. Each one names what to do next, because an
/// operator standing at the second machine cannot read the core's log.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Refusal {
    Unknown,
    Used(u64),
    Expired(u64),
    WrongName { minted_for: String, asked_for: String },
}

impl std::fmt::Display for Refusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Refusal::Unknown => write!(
                f,
                "that enrolment token is not one this core minted. Run `gmx node token --name \
                 <node>` on the core and use the string it prints"
            ),
            Refusal::Used(at) => write!(
                f,
                "that enrolment token was already used at unix time {at}. A token is good for \
                 one node. Mint another with `gmx node token --name <node>`"
            ),
            Refusal::Expired(at) => write!(
                f,
                "that enrolment token expired at unix time {at}. Mint another with `gmx node \
                 token --name <node>`"
            ),
            Refusal::WrongName { minted_for, asked_for } => write!(
                f,
                "that enrolment token was minted for the node `{minted_for}` and this machine \
                 called itself `{asked_for}`. Start the node with `--name {minted_for}`, or mint \
                 a token for `{asked_for}`"
            ),
        }
    }
}

impl std::error::Error for Refusal {}

/// The core's book of minted tokens, kept as JSON beside the CA.
pub struct Tickets {
    path: PathBuf,
    entries: Mutex<Vec<Ticket>>,
}

impl Tickets {
    /// Open the book at `path`, making an empty one if it is not there.
    ///
    /// A file that will not parse is not fatal and is not silently thrown
    /// away: it is moved aside, so the core starts and the operator can see
    /// what was there.
    pub fn open(path: &Path) -> Self {
        let entries = match std::fs::read_to_string(path) {
            Ok(text) => match serde_json::from_str::<Vec<Ticket>>(&text) {
                Ok(list) => list,
                Err(e) => {
                    let aside = path.with_extension("json.broken");
                    tracing::warn!(
                        path = %path.display(),
                        moved = %aside.display(),
                        ?e,
                        "the enrolment token file would not parse; moving it aside and starting \
                         with an empty one"
                    );
                    let _ = std::fs::rename(path, &aside);
                    Vec::new()
                }
            },
            Err(_) => Vec::new(),
        };
        Self { path: path.to_path_buf(), entries: Mutex::new(entries) }
    }

    /// An in memory book, for tests and for a core with no runtime directory.
    pub fn detached() -> Self {
        Self { path: PathBuf::new(), entries: Mutex::new(Vec::new()) }
    }

    /// Mint a token for one node.
    pub fn mint(&self, name: &str, ttl: Duration) -> Result<Ticket> {
        let now = now_unix();
        let ticket = Ticket {
            token: secret()?,
            name: name.to_string(),
            minted_unix: now,
            expires_unix: now + ttl.as_secs().max(1),
            used_unix: None,
        };
        {
            let mut entries = self.entries.lock();
            entries.push(ticket.clone());
            // Forget tokens that expired a day ago. The book is a record of
            // what is live, not an audit log; the session log is the audit log.
            entries.retain(|t| t.expires_unix + 86_400 > now);
        }
        self.save();
        Ok(ticket)
    }

    /// Spend a token, or say why not.
    ///
    /// The comparison walks every entry without an early exit, the way
    /// `Tokens::authenticate` does, so the time it takes says nothing about
    /// how much of the token was right.
    pub fn redeem(&self, name: &str, presented: &str) -> Result<Ticket, Refusal> {
        let now = now_unix();
        let mut entries = self.entries.lock();
        let mut found: Option<usize> = None;
        for (i, ticket) in entries.iter().enumerate() {
            if godwinmix_protocol::scope::constant_time_eq(
                ticket.token.as_bytes(),
                presented.as_bytes(),
            ) {
                found = Some(i);
            }
        }
        let Some(i) = found else { return Err(Refusal::Unknown) };
        if let Some(at) = entries[i].used_unix {
            return Err(Refusal::Used(at));
        }
        if entries[i].expired(now) {
            return Err(Refusal::Expired(entries[i].expires_unix));
        }
        if entries[i].name != name {
            return Err(Refusal::WrongName {
                minted_for: entries[i].name.clone(),
                asked_for: name.to_string(),
            });
        }
        entries[i].used_unix = Some(now);
        let spent = entries[i].clone();
        drop(entries);
        self.save();
        Ok(spent)
    }

    /// Every token this core knows about, secrets blanked.
    pub fn list(&self) -> Vec<Ticket> {
        self.entries
            .lock()
            .iter()
            .map(|t| Ticket { token: String::new(), ..t.clone() })
            .collect()
    }

    /// Forget every token minted for one node. Called by `node.remove`, so a
    /// node that was removed cannot walk back in on a token that was minted
    /// before it left.
    pub fn forget(&self, name: &str) -> usize {
        let removed = {
            let mut entries = self.entries.lock();
            let before = entries.len();
            entries.retain(|t| t.name != name);
            before - entries.len()
        };
        if removed > 0 {
            self.save();
        }
        removed
    }

    fn save(&self) {
        if self.path.as_os_str().is_empty() {
            return;
        }
        let text = { serde_json::to_string_pretty(&*self.entries.lock()).unwrap_or_default() };
        if let Some(parent) = self.path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Err(e) = write_private(&self.path, &text) {
            tracing::warn!(path = %self.path.display(), ?e, "could not write the enrolment tokens");
        }
    }
}

/// 32 bytes of randomness as lower case hex. Not a slug: nobody types this
/// from memory and nobody should be able to guess the next one.
pub fn secret() -> Result<String> {
    let mut bytes = [0u8; 32];
    getrandom::fill(&mut bytes).context("the operating system would not give us random bytes")?;
    Ok(bytes.iter().map(|b| format!("{b:02x}")).collect())
}

pub fn now_unix() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

fn write_private(path: &Path, contents: &str) -> Result<()> {
    std::fs::write(path, contents)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_token_works_once() {
        let book = Tickets::detached();
        let t = book.mint("studio-b", DEFAULT_TTL).unwrap();
        assert!(book.redeem("studio-b", &t.token).is_ok());
        match book.redeem("studio-b", &t.token) {
            Err(Refusal::Used(_)) => {}
            other => panic!("a second use must be refused, got {other:?}"),
        }
    }

    #[test]
    fn an_expired_token_is_refused() {
        let book = Tickets::detached();
        let t = book.mint("studio-b", Duration::from_secs(1)).unwrap();
        // Reach in and age it rather than sleeping: the refusal is what is
        // under test, not the passage of time.
        book.entries.lock()[0].expires_unix = now_unix() - 1;
        match book.redeem("studio-b", &t.token) {
            Err(Refusal::Expired(_)) => {}
            other => panic!("an expired token must be refused, got {other:?}"),
        }
    }

    #[test]
    fn a_token_is_bound_to_its_node() {
        let book = Tickets::detached();
        let t = book.mint("studio-b", DEFAULT_TTL).unwrap();
        match book.redeem("studio-c", &t.token) {
            Err(Refusal::WrongName { minted_for, .. }) => assert_eq!(minted_for, "studio-b"),
            other => panic!("a token minted for another node must be refused, got {other:?}"),
        }
    }

    #[test]
    fn an_invented_token_is_refused() {
        let book = Tickets::detached();
        book.mint("studio-b", DEFAULT_TTL).unwrap();
        assert_eq!(book.redeem("studio-b", "deadbeef"), Err(Refusal::Unknown));
    }

    #[test]
    fn removing_a_node_forgets_its_tokens() {
        let book = Tickets::detached();
        let t = book.mint("studio-b", DEFAULT_TTL).unwrap();
        assert_eq!(book.forget("studio-b"), 1);
        assert_eq!(book.redeem("studio-b", &t.token), Err(Refusal::Unknown));
    }

    #[test]
    fn every_refusal_names_the_next_step() {
        let all = [
            Refusal::Unknown,
            Refusal::Used(1),
            Refusal::Expired(1),
            Refusal::WrongName { minted_for: "a".into(), asked_for: "b".into() },
        ];
        for r in all {
            let text = r.to_string();
            assert!(
                text.contains("gmx node token") || text.contains("--name"),
                "a refusal must say what to do next: {text}"
            );
        }
    }
}
