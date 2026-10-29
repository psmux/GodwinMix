//! Guests: a browser's camera and microphone taken in over WHIP straight into
//! this show, and the `guest/source` that puts them in the mixer.
//!
//! ```text
//!   the guest's browser ──WHIP──► session pipeline (one per guest, session.rs)
//!        webrtcbin ──► depay ──► decode ──► intervideosink / interaudiosink
//!                                                   │  channel gmx-guest-<id>
//!   guest/source (source.rs)                        ▼
//!        intervideosrc / interaudiosrc ──► the shared normaliser ──► the mixer
//! ```
//!
//! Two pipelines on purpose. The source is built once, by the mixer, and
//! stays up whatever the guest's network does: a reload, a phone moving from
//! Wi-Fi to cellular or a laptop going to sleep ends one session and starts
//! another, and the source in the scene never notices beyond holding its last
//! picture. The session pipeline is built when an offer arrives, on the HTTP
//! handler's blocking thread, and taken down by the watcher or by a revoke,
//! never on a streaming thread and never on the mixer thread.
//!
//! Nothing runs unless asked: no offer, no session; no `guest/source`, no
//! inter source. The return the guest hears and sees is not here: it is a
//! `whep/output` whose sound is a mix minus bus (see `crate::buses`), which
//! the binary adds when the guest first offers.

mod branch;
mod session;
pub mod source;
mod watch;

#[cfg(test)]
mod tests;

pub use crate::whep::Refusal;
use parking_lot::{Mutex, RwLock};
use session::Session;
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, OnceLock};

/// Where a guest is on its way in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Link {
    /// No session has been offered yet.
    Waiting,
    /// An offer was answered and the connection is being made.
    Joining,
    /// The peer connection is up and media is arriving.
    Live,
    /// The last session ended: the guest closed the page or lost the network.
    Gone,
}

/// What a session's media is made into before it crosses to the source.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Shape {
    pub fps: i32,
    pub rate: i32,
    pub channels: i32,
}

struct Guest {
    session: Mutex<Option<(String, Session)>>,
    link: Mutex<Link>,
    next: AtomicU64,
}

fn registry() -> &'static RwLock<BTreeMap<String, Arc<Guest>>> {
    static GUESTS: OnceLock<RwLock<BTreeMap<String, Arc<Guest>>>> = OnceLock::new();
    GUESTS.get_or_init(Default::default)
}

fn guest(id: &str) -> Arc<Guest> {
    if let Some(g) = registry().read().get(id) {
        return g.clone();
    }
    let fresh = Guest { session: Mutex::new(None), link: Mutex::new(Link::Waiting), next: AtomicU64::new(1) };
    registry().write().entry(id.to_string()).or_insert_with(|| Arc::new(fresh)).clone()
}

type Notify = Box<dyn Fn(&str) + Send + Sync>;

/// Called with a guest's id whenever its [`Link`] changes. Set once, by the
/// binary, which turns it into an event; until then a change is only kept.
pub fn on_change(notify: Notify) {
    let _ = NOTIFY.set(notify);
}

static NOTIFY: OnceLock<Notify> = OnceLock::new();

pub(crate) fn set_link(id: &str, link: Link) {
    let changed = {
        // A guest that was hung up is not brought back by a late watcher.
        let Some(g) = registry().read().get(id).cloned() else { return };
        let mut now = g.link.lock();
        std::mem::replace(&mut *now, link) != link
    };
    if let (true, Some(notify)) = (changed, NOTIFY.get()) {
        notify(id);
    }
}

/// Where the guest `id` is now.
pub fn link(id: &str) -> Link {
    registry().read().get(id).map(|g| *g.link.lock()).unwrap_or(Link::Waiting)
}

/// Whether a source is a guest's, which the runtime store never keeps: the
/// guests module adds it again when the guest offers.
pub fn made_for_a_guest_source(cfg: &crate::config::SourceConfig) -> bool {
    source::guest_of(&cfg.uri).is_some()
}

/// The param that marks a `whep/output` as a guest's return.
pub const RETURN_PARAM: &str = "guest";

/// Whether an output is a guest's return, which the runtime store never keeps.
pub fn made_for_a_guest_output(cfg: &crate::config::OutputConfig) -> bool {
    cfg.params.contains_key(RETURN_PARAM)
}

/// The inter channel a guest's session writes and its source reads.
pub fn channel(id: &str) -> String {
    format!("gmx-guest-{id}")
}

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
