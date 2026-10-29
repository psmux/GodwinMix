//! Making, revoking and running out: the invite's own lifecycle.

use super::store::Record;
use super::{now_ms, Guests};
use crate::channels::keys::slug;
use godwinmix_protocol::error::RpcError;
use godwinmix_protocol::guests::*;
use std::sync::Arc;
use std::time::Duration;

/// How long a key is: 32 letters from a 32 letter alphabet, 160 bits.
const KEY_LEN: usize = 32;
/// The most invites one show keeps, live or not.
const MAX_INVITES: usize = 64;

impl Guests {
    pub async fn create(self: &Arc<Self>, req: InviteCreateRequest) -> Result<InviteCreated, RpcError> {
        let name = req.name.trim().to_string();
        if name.is_empty() || name.chars().count() > 64 {
            return Err(RpcError::invalid_params("name is what the guest is called on screen: 1 to 64 characters, such as Ana Lopez.").with("field", "name"));
        }
        let secs = req.expires_in_secs.unwrap_or(DEFAULT_EXPIRES_SECS);
        if !(60..=MAX_EXPIRES_SECS).contains(&secs) {
            let msg = format!("expires_in_secs is {secs}; an invite lasts from 60 seconds to {MAX_EXPIRES_SECS} (thirty days). Leave it out for one day.");
            return Err(RpcError::invalid_params(msg).with("field", "expires_in_secs").with("max", MAX_EXPIRES_SECS));
        }
        if let Some(b) = req.base.as_deref().map(str::trim).filter(|b| !b.is_empty() && !b.starts_with("http://") && !b.starts_with("https://")) {
            let msg = format!("base is '{b}'; it is the address the link starts with, such as https://studio.example.com. Leave it out for this machine's own.");
            return Err(RpcError::invalid_params(msg).with("field", "base"));
        }
        let key = godwinmix_core::secrets::random_key(KEY_LEN).map_err(|e| RpcError::internal(format!("{e:#}")))?;
        let record = {
            let mut invites = self.invites.lock();
            if invites.len() >= MAX_INVITES {
                let msg = format!("this show already has {MAX_INVITES} invites. Revoke the ones nobody will use with guest.invite.revoke, then make this one.");
                return Err(RpcError::not_in_state(msg).with("max", MAX_INVITES));
            }
            let id = pick_id(req.id.as_deref(), &name, |id| invites.contains_key(id))?;
            let created = crate::channels::keys::now();
            let record = Record { id: id.clone(), name, created, expires_ms: now_ms() + secs * 1000, key };
            invites.insert(id, record.clone());
            record
        };
        if let Err(why) = self.save() {
            self.invites.lock().remove(&record.id);
            return Err(RpcError::internal(format!("the invite could not be saved ({why}). Make the folder writable and try again; nothing was made.")).with("file", why));
        }
        self.arm();
        self.changed();
        let link = self.link_of(&record, req.base.as_deref())?;
        Ok(InviteCreated { invite: super::invite_of(&record, false), link })
    }

    /// Take an invite back: its guest's stream and return end at once, and
    /// the source, the bus and the return output go.
    pub async fn revoke(self: &Arc<Self>, id: &str) -> Result<InviteRevoked, RpcError> {
        let Some(record) = self.invites.lock().remove(id) else { return Err(self.no_invite(id)) };
        let held = self.talk.lock().held.remove(id);
        let revoked = super::invite_of(&record, held);
        godwinmix_core::guest::hang_up(id);
        self.leave(id).await;
        if let Err(why) = self.save() {
            tracing::error!(guest = %id, %why, "a revoked invite could not be written out; it is gone until a restart reads the old file");
        }
        self.changed();
        Ok(InviteRevoked { revoked })
    }

    pub(super) fn no_invite(&self, id: &str) -> RpcError {
        let valid: Vec<String> = self.invites.lock().keys().cloned().collect();
        RpcError::not_found("invite", id, &valid)
    }

    /// Sleep until the next invite runs out, revoke it, and go round again.
    /// One task at most, and none while there is nothing to wait for.
    pub(super) fn arm(self: &Arc<Self>) {
        let Some(next) = self.invites.lock().values().map(|r| r.expires_ms).min() else { return };
        {
            let mut armed = self.armed.lock();
            if armed.is_some_and(|at| at <= next) {
                return;
            }
            *armed = Some(next);
        }
        let Ok(runtime) = tokio::runtime::Handle::try_current() else { return };
        let me = self.clone();
        runtime.spawn(async move {
            tokio::time::sleep(Duration::from_millis(next.saturating_sub(now_ms()))).await;
            *me.armed.lock() = None;
            let due: Vec<String> = me.invites.lock().values().filter(|r| r.expires_ms <= now_ms()).map(|r| r.id.clone()).collect();
            for id in due {
                tracing::info!(guest = %id, "an invite ran out; its guest was let go");
                let _ = me.revoke(&id).await;
            }
            me.arm();
        });
    }
}

/// The id asked for, checked, or the name's slug with a number on the end
/// when that is taken.
fn pick_id(asked: Option<&str>, name: &str, taken: impl Fn(&str) -> bool) -> Result<String, RpcError> {
    let reserved = |id: &str| id == TALKBACK || taken(id);
    let Some(asked) = asked else {
        let base = Some(slug(name)).filter(|s| !s.is_empty()).unwrap_or_else(|| "guest".into());
        return Ok(crate::channels::keys::free(&base, reserved));
    };
    let wanted = slug(asked);
    if wanted != asked || asked.len() > 40 {
        let msg = format!("'{asked}' is not an id: lower case letters, digits and single dashes, at most 40, starting with a letter. '{wanted}' would do, or leave id out.");
        return Err(RpcError::invalid_params(msg).with("field", "id").with("suggestion", wanted));
    }
    if reserved(asked) {
        let msg = format!("there is already an invite called '{asked}' (or it is the operator's talkback). Pick another id, or leave id out.");
        return Err(RpcError::invalid_params(msg).with("field", "id"));
    }
    Ok(asked.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_id_is_the_name_as_a_slug_and_never_talkback() {
        assert_eq!(pick_id(None, "Ana Lopez", |_| false).unwrap(), "ana-lopez");
        assert_eq!(pick_id(None, "Talkback", |_| false).unwrap(), "talkback-2");
        assert_eq!(pick_id(None, "Ana", |id| id == "ana").unwrap(), "ana-2");
        assert_eq!(pick_id(Some("pastor"), "Rev. Smith", |_| false).unwrap(), "pastor");
        let bad = pick_id(Some("Pastor Smith"), "x", |_| false).unwrap_err();
        assert!(bad.message.contains("pastor-smith"), "{}", bad.message);
        assert!(pick_id(Some("talkback"), "x", |_| false).is_err());
    }
}
