//! Basic auth as Flussonic does it: `edit_auth` may read and change,
//! `view_auth`, when it is set, may read.

use crate::model::{Config, Credentials};
use base64::Engine;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Access {
    Read,
    Edit,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Refusal {
    /// No credentials, or the wrong ones: 401 with a challenge.
    Unauthorized,
    /// Right credentials for reading, and this is a change: 403.
    ReadOnly,
    /// The config has no `edit_auth`, and the call is not from this machine.
    NoAuthConfigured,
}

/// The user and password of an `Authorization: Basic ...` header.
pub fn basic(header: Option<&str>) -> Option<(String, String)> {
    let b64 = header?.trim().strip_prefix("Basic ").or_else(|| header?.trim().strip_prefix("basic "))?;
    let raw = base64::engine::general_purpose::STANDARD.decode(b64.trim()).ok()?;
    let text = String::from_utf8(raw).ok()?;
    let (u, p) = text.split_once(':')?;
    Some((u.to_string(), p.to_string()))
}

/// Compare without stopping at the first byte that differs.
fn same(a: &str, b: &str) -> bool {
    let (a, b) = (a.as_bytes(), b.as_bytes());
    let mut diff = (a.len() != b.len()) as u8;
    for i in 0..a.len().max(b.len()) {
        diff |= a.get(i).copied().unwrap_or(0) ^ b.get(i).copied().unwrap_or(0);
    }
    diff == 0
}

fn matches(c: &Option<Credentials>, given: &(String, String)) -> bool {
    c.as_ref().is_some_and(|c| same(&c.user, &given.0) & same(&c.password, &given.1))
}

/// Whether a call with this `Authorization` header may do `want`. A config
/// with no `edit_auth` lets in calls from this machine only.
pub fn check(cfg: &Config, header: Option<&str>, want: Access, local: bool) -> Result<(), Refusal> {
    if cfg.edit_auth.is_none() {
        return if local { Ok(()) } else { Err(Refusal::NoAuthConfigured) };
    }
    let Some(given) = basic(header) else { return Err(Refusal::Unauthorized) };
    if matches(&cfg.edit_auth, &given) {
        return Ok(());
    }
    if matches(&cfg.view_auth, &given) {
        return if want == Access::Read { Ok(()) } else { Err(Refusal::ReadOnly) };
    }
    Err(Refusal::Unauthorized)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Document;

    fn header(u: &str, p: &str) -> String {
        format!("Basic {}", base64::engine::general_purpose::STANDARD.encode(format!("{u}:{p}")))
    }

    #[test]
    fn edit_reads_and_writes_and_view_only_reads() {
        let cfg = Document::parse("edit_auth admin secret;\nview_auth viewer look;\n").unwrap().config;
        assert_eq!(check(&cfg, Some(&header("admin", "secret")), Access::Edit, false), Ok(()));
        assert_eq!(check(&cfg, Some(&header("viewer", "look")), Access::Read, false), Ok(()));
        assert_eq!(check(&cfg, Some(&header("viewer", "look")), Access::Edit, false), Err(Refusal::ReadOnly));
        assert_eq!(check(&cfg, Some(&header("admin", "wrong")), Access::Read, true), Err(Refusal::Unauthorized));
        assert_eq!(check(&cfg, None, Access::Read, true), Err(Refusal::Unauthorized));
        let open = Document::parse("http 80;\n").unwrap().config;
        assert_eq!(check(&open, None, Access::Edit, true), Ok(()));
        assert_eq!(check(&open, None, Access::Read, false), Err(Refusal::NoAuthConfigured));
    }
}
