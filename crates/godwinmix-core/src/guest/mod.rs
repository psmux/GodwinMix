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
mod calls;
mod session;
pub mod source;
mod watch;

#[cfg(test)]
mod tests;

pub use calls::{end, hang_up, offer};
pub use crate::whep::Refusal;
use parking_lot::{Mutex, RwLock};
use session::Session;
use std::collections::BTreeMap;
use std::sync::atomic::AtomicU64;
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
    pub fps: crate::config::FrameRate,
    pub rate: i32,
    pub channels: i32,
}

pub(crate) struct Guest {
    pub(crate) session: Mutex<Option<(String, Session)>>,
    link: Mutex<Link>,
    pub(crate) next: AtomicU64,
}

pub(crate) fn registry() -> &'static RwLock<BTreeMap<String, Arc<Guest>>> {
    static GUESTS: OnceLock<RwLock<BTreeMap<String, Arc<Guest>>>> = OnceLock::new();
    GUESTS.get_or_init(Default::default)
}

pub(crate) fn guest(id: &str) -> Arc<Guest> {
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
