//! An `ingest/rtmp` source that owns its port: one publisher at a time,
//! straight into the remuxer.
//!
//! # A publisher that comes back
//!
//! What goes down stdout is one Matroska stream with one set of codec
//! headers and one timeline, and a second publisher's would be spliced onto
//! the end of it, which no demuxer accepts (`relayed.rs` says the same of a
//! channel stream). So the first publisher whose picture reached the
//! remuxer is the last one this process carries. When it leaves, by hanging
//! up, by [`IDLE`](crate::rtmp::IDLE) of silence after a pulled cable, or by
//! being taken over, the stream is ended and the process exits. The core
//! restarts it in place, behind its freeze frame, and the new process listens
//! on the same port within a second: the encoder's reconnect lands there on a
//! clean pipe. A publisher that left before its first keyframe wrote nothing,
//! so the port simply takes the next one.
//!
//! A publisher that has sent nothing for [`STALE`] is taken over at once by
//! a newcomer, rather than holding the port until its connection times out.
//! It is cut off; if its picture had reached the remuxer the newcomer is
//! asked to publish again in a second, which is how long the clean restart
//! takes, and otherwise it is let in there and then.

mod inlet;

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

use godwinmix_sdk::plugin::Reporter;
use godwinmix_sdk::wire::Health;

use super::{Settings, State};
use crate::channels::split_query;
use crate::hub::STALE;
use crate::remux::Remux;
use crate::rtmp::{Filter, Gate, Inlet, Kick, Server};
use inlet::{Quiet, ToRemux};

/// What to do once a publisher's stream has ended: finish the stream and
/// exit, in a real source process.
pub type End = Arc<dyn Fn(&Remux) + Send + Sync>;

pub fn start(settings: &Settings, reporter: Option<Reporter>, remux: Remux, state: Arc<State>, end: End) -> Result<Server, String> {
    let gate = gate(settings, reporter.clone(), remux, state.clone(), end);
    let server = Server::bind(&settings.bind, settings.port, gate)?;
    let port = server.port();
    state.port.store(port, Ordering::Relaxed);
    state.set_address(settings.publish_url(port));
    if let Some(r) = &reporter {
        r.info(format!("waiting for a publisher at {}", state.address()));
    }
    Ok(server)
}

fn gate(settings: &Settings, reporter: Option<Reporter>, remux: Remux, state: Arc<State>, end: End) -> Arc<OneAtATime> {
    let filter = Filter { app: settings.app.clone(), key: settings.stream_key.clone() };
    let (held, spent, next) = (Mutex::new(None), AtomicBool::new(false), AtomicU64::new(1));
    Arc::new(OneAtATime(Arc::new(Inner { filter, remux, state, reporter, held, spent, next, end })))
}

/// A source is one picture, so a second publisher is refused rather than
/// silently switched to, unless the first has gone quiet.
struct OneAtATime(Arc<Inner>);

struct Inner {
    filter: Filter,
    remux: Remux,
    state: Arc<State>,
    reporter: Option<Reporter>,
    /// The publisher on the port, if any.
    held: Mutex<Option<Holder>>,
    /// Set once a publisher's picture has reached the remuxer: nothing else
    /// may, and the process exits when that publisher goes.
    spent: AtomicBool,
    next: AtomicU64,
    end: End,
}

struct Holder {
    id: u64,
    kick: Kick,
    quiet: Arc<Quiet>,
}

impl Inner {
    fn held(&self) -> MutexGuard<'_, Option<Holder>> {
        self.held.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn refuse(&self, app: &str, key: &str, why: String) -> Result<Box<dyn Inlet>, String> {
        self.note(format!("refused '{app}/{key}': {why}"));
        Err(why)
    }

    fn note(&self, message: String) {
        if let Some(r) = &self.reporter {
            r.warn(message);
        }
    }

    fn health(&self, health: Health) {
        if let Some(r) = &self.reporter {
            r.health_changed(health);
        }
    }
}

const BUSY: &str = "this source already has a publisher. One RTMP source carries one picture; \
                    add a second ingest/rtmp source on another port, or create a channel, which \
                    takes many publishers on one port.";

const AGAIN: &str = "this source is starting a clean stream after the last publisher, which \
                     takes about a second. Publish again; an encoder set to reconnect does so by itself.";

impl Gate for OneAtATime {
    fn admit(&self, app_raw: &str, stream_raw: &str, peer: &str, kick: Kick) -> Result<Box<dyn Inlet>, String> {
        let me = &self.0;
        let (app, _) = split_query(app_raw);
        let key = stream_raw.trim();
        if !me.filter.accepts(app, key) {
            return me.refuse(app, key, me.filter.refusal(app, key));
        }
        let mut held = me.held();
        if let Some(old) = held.as_ref() {
            if old.quiet.quiet() < STALE {
                return me.refuse(app, key, BUSY.to_string());
            }
            let old = held.take().map(|h| h.kick);
            drop(held);
            if let Some(kick) = old {
                me.note(format!("the publisher before had sent nothing for {} s, so {peer} took the port over", STALE.as_secs()));
                kick();
            }
            held = me.held();
        }
        if me.spent.load(Ordering::Acquire) {
            return me.refuse(app, key, AGAIN.to_string());
        }
        let quiet = Arc::new(Quiet::new());
        let id = me.next.fetch_add(1, Ordering::Relaxed);
        *held = Some(Holder { id, kick, quiet: quiet.clone() });
        drop(held);
        let who = format!("{app}/{key} from {peer}");
        me.state.set_publisher(Some(who.clone()));
        if let Some(r) = &me.reporter {
            r.info(format!("{who} started publishing"));
        }
        let mut health = Health::ok();
        health.detail = Some(format!("{who} is publishing"));
        me.health(health);
        Ok(Box::new(ToRemux { id, name: format!("{app}/{key}"), wrote: false, quiet, gate: me.clone() }))
    }

    fn note(&self, message: String) {
        self.0.note(message);
    }
}

#[cfg(test)]
#[path = "listen/tests.rs"]
mod tests;
