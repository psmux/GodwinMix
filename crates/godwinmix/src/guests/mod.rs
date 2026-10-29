//! Guest invites: a link with a key of its own for one person to join the
//! show from a browser, see and hear a return made for them, and be talked
//! to off air.
//!
//! ```text
//!   guest.invite.create ──► Record (key, expiry) ──► <stem>.guests.json
//!   the guest's browser ── POST /guest/<id>/whip ──► join.rs: source, bus, return output
//!                                                  └─► godwinmix_core::guest::offer
//!                       ── POST /guest/<id>/whep ──► the return: programme picture, mix minus sound
//!   guest.talkback.set ──► talk.rs ──► the desk holds the talkback source on the guest's bus
//! ```
//!
//! What it costs while nobody is invited: nothing. No task and no timer. An
//! invite on its own costs a line in a file. The source, the mix minus bus
//! and the return output are made when the guest first offers and go with
//! the invite, so a guest who never opens the link never costs the mixer a
//! thread. The one timer is the expiry, a sleeping task armed while some
//! invite has a live guest or was just made.

mod create;
mod http;
mod join;
mod link;
mod qr;
mod qr_grid;
mod store;
mod talk;
#[cfg(test)]
mod qr_tests;
#[cfg(test)]
mod tests;

pub use http::router;

use godwinmix_core::guest::{Link, Shape};
use godwinmix_core::mixer::MixerHandle;
use godwinmix_protocol::guests::{GuestList, GuestState, Invite, MAX_GUESTS};
use godwinmix_protocol::types::Event;
use parking_lot::Mutex;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Weak};
use store::Record;

pub struct Guests {
    invites: Mutex<BTreeMap<String, Record>>,
    /// `<stem>.guests.json` beside the config. None for a core with no config
    /// file, which keeps its invites for as long as it runs.
    file: Option<PathBuf>,
    mixer: MixerHandle,
    shape: Shape,
    talk: Mutex<talk::Talk>,
    /// The guests whose source, bus and return are built now.
    joined: Mutex<BTreeSet<String>>,
    /// Serialises the building and taking down of one guest's pieces.
    building: tokio::sync::Mutex<()>,
    armed: Mutex<Option<u64>>,
}

/// The mixer's clock, in milliseconds since the Unix epoch.
pub fn now_ms() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

/// The source a guest's camera is.
pub fn source_of(id: &str) -> String {
    format!("guest-{id}")
}

/// The output a guest's return is served from.
pub fn return_of(id: &str) -> String {
    format!("guest-{id}-return")
}

impl Guests {
    /// Read the invites kept beside `config`. Starts nothing.
    pub fn open(config: &Path, mixer: MixerHandle, shape: Shape) -> Arc<Guests> {
        let file = (!config.as_os_str().is_empty()).then(|| store::path_beside(config));
        let kept = file.as_deref().map(|f| store::load(f, now_ms())).unwrap_or_default();
        let invites = kept.into_iter().map(|r| (r.id.clone(), r)).collect();
        let guests = Arc::new(Guests {
            invites: Mutex::new(invites),
            file,
            mixer,
            shape,
            talk: Mutex::new(talk::Talk::default()),
            joined: Mutex::new(BTreeSet::new()),
            building: tokio::sync::Mutex::new(()),
            armed: Mutex::new(None),
        });
        let weak: Weak<Guests> = Arc::downgrade(&guests);
        godwinmix_core::guest::on_change(Box::new(move |_| {
            if let Some(g) = weak.upgrade() {
                g.changed();
            }
        }));
        guests
    }

    /// Every invite and where its guest is, without any key.
    pub fn list(&self) -> GuestList {
        let held = self.talk.lock().held.clone();
        let guests = self.invites.lock().values().map(|r| invite_of(r, held.contains(&r.id))).collect();
        GuestList { guests, talkback: self.talk_state(), max_guests: MAX_GUESTS }
    }

    /// Tell every subscriber.
    fn changed(&self) {
        self.mixer.emit(Event::GuestChanged { list: Box::new(self.list()) });
    }

    fn save(&self) -> Result<(), String> {
        let Some(path) = &self.file else { return Ok(()) };
        let all: Vec<Record> = self.invites.lock().values().cloned().collect();
        store::write(path, &all)
    }

    /// How many guests are joining or live, leaving `except` out.
    fn in_use(&self, except: &str) -> usize {
        let ids: Vec<String> = self.invites.lock().keys().filter(|id| *id != except).cloned().collect();
        ids.iter().filter(|id| matches!(godwinmix_core::guest::link(id), Link::Joining | Link::Live)).count()
    }
}

fn invite_of(r: &Record, talkback: bool) -> Invite {
    let state = match godwinmix_core::guest::link(&r.id) {
        Link::Waiting => GuestState::Invited,
        Link::Joining => GuestState::Joining,
        Link::Live => GuestState::Live,
        Link::Gone => GuestState::Gone,
    };
    let expires = std::time::UNIX_EPOCH + std::time::Duration::from_millis(r.expires_ms);
    Invite {
        id: r.id.clone(),
        name: r.name.clone(),
        created: r.created.clone(),
        expires: godwinmix_core::observe::logs::rfc3339(&expires),
        state,
        source: source_of(&r.id),
        bus: source_of(&r.id),
        talkback,
    }
}
