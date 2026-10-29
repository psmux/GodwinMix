//! The calls on one guest: an offer, a session ended, a guest hung up.
//! None of them holds the registry's lock while a session is negotiated or
//! taken down, which can each take a few seconds.

use super::session::Session;
use super::{guest, registry, set_link, Link, Refusal, Shape};
use std::sync::atomic::Ordering;

/// A guest's offer: the session id and the SDP answer. A session the guest
/// already had is ended first, so a page reloaded or a phone that changed
/// network is back at once. Blocks for up to three seconds while ICE
/// gathers, so call it off the async runtime.
pub fn offer(id: &str, sdp: &str, shape: Shape, ice: &crate::preview::whep::IceConfig) -> Result<(String, String), Refusal> {
    let g = guest(id);
    let old = g.session.lock().take();
    drop(old);
    // Random as well as numbered: ending a session takes only its name.
    let tail = crate::secrets::random_key(12).map_err(|e| Refusal::internal(format!("{e:#}")))?;
    let name = format!("s{}-{tail}", g.next.fetch_add(1, Ordering::Relaxed));
    let (session, answer) = Session::start(id, &name, sdp, shape, ice)?;
    let stale = g.session.lock().replace((name.clone(), session));
    drop(stale);
    set_link(id, Link::Joining);
    Ok((name, answer))
}

/// The guest said it is done (`DELETE`). False when there was no session by
/// that name.
pub fn end(id: &str, session: &str) -> bool {
    let Some(g) = registry().read().get(id).cloned() else { return false };
    let gone = {
        let mut held = g.session.lock();
        if held.as_ref().is_some_and(|(name, _)| name == session) {
            held.take()
        } else {
            None
        }
    };
    let ended = gone.is_some();
    drop(gone);
    if ended {
        set_link(id, Link::Gone);
    }
    ended
}

/// End whatever session the guest has and forget it, for a revoked invite.
pub fn hang_up(id: &str) {
    let gone = registry().write().remove(id);
    if let Some(g) = gone {
        let session = g.session.lock().take();
        drop(session);
    }
}

/// Called by the watcher when a session's peer has gone for good. Ends it
/// only when it is still the guest's current one.
pub(crate) fn lost(id: &str, session: &str) {
    if end(id, session) {
        tracing::info!(guest = %id, %session, "a guest's connection ended");
    }
}
