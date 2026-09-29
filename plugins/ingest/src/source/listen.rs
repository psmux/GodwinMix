//! An `ingest/rtmp` source that owns its port: one publisher at a time,
//! straight into the remuxer.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use godwinmix_sdk::plugin::Reporter;
use godwinmix_sdk::wire::Health;

use super::{Settings, State};
use crate::channels::split_query;
use crate::flv;
use crate::media_tag::{MediaTag, TagKind};
use crate::remux::Remux;
use crate::rtmp::{Filter, Gate, Inlet, Kick, Server};

pub fn start(
    settings: &Settings,
    reporter: Option<Reporter>,
    remux: Remux,
    state: Arc<State>,
) -> Result<Server, String> {
    let gate = Arc::new(OneAtATime {
        filter: Filter { app: settings.app.clone(), key: settings.stream_key.clone() },
        busy: Arc::new(AtomicBool::new(false)),
        remux: Arc::new(remux),
        state: state.clone(),
        reporter: reporter.clone(),
    });
    let server = Server::bind(&settings.bind, settings.port, gate)?;
    let port = server.port();
    state.port.store(port, Ordering::Relaxed);
    state.set_address(settings.publish_url(port));
    if let Some(r) = &reporter {
        r.info(format!("waiting for a publisher at {}", state.address()));
    }
    Ok(server)
}

/// A source is one picture, so a second publisher is refused rather than
/// silently switched to.
struct OneAtATime {
    filter: Filter,
    busy: Arc<AtomicBool>,
    remux: Arc<Remux>,
    state: Arc<State>,
    reporter: Option<Reporter>,
}

impl Gate for OneAtATime {
    fn admit(&self, app_raw: &str, stream_raw: &str, peer: &str, _: Kick) -> Result<Box<dyn Inlet>, String> {
        let (app, _) = split_query(app_raw);
        let key = stream_raw.trim();
        if !self.filter.accepts(app, key) {
            let why = self.filter.refusal(app, key);
            self.note(format!("refused '{app}/{key}': {why}"));
            return Err(why);
        }
        if self.busy.compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire).is_err() {
            let why = "this source already has a publisher. One RTMP source carries one \
                       picture; add a second ingest/rtmp source on another port, or create a \
                       channel, which takes many publishers on one port."
                .to_string();
            self.note(format!("refused '{app}/{key}': {why}"));
            return Err(why);
        }
        let who = format!("{app}/{key} from {peer}");
        self.state.set_publisher(Some(who.clone()));
        if let Some(r) = &self.reporter {
            r.info(format!("{who} started publishing"));
            let mut health = Health::ok();
            health.detail = Some(format!("{who} is publishing"));
            r.health_changed(health);
        }
        let header = flv::header();
        self.remux.write(&header);
        self.state.wrote(header.len(), &self.remux);
        Ok(Box::new(ToRemux {
            name: format!("{app}/{key}"),
            seen_keyframe: false,
            busy: self.busy.clone(),
            remux: self.remux.clone(),
            state: self.state.clone(),
            reporter: self.reporter.clone(),
        }))
    }

    fn note(&self, message: String) {
        if let Some(r) = &self.reporter {
            r.warn(message);
        }
    }
}

/// The one publisher's tags, written on as FLV.
struct ToRemux {
    name: String,
    /// Nothing is written until the first keyframe, so a decoder is never
    /// handed a run of inter frames with nothing to decode them against.
    seen_keyframe: bool,
    busy: Arc<AtomicBool>,
    remux: Arc<Remux>,
    state: Arc<State>,
    reporter: Option<Reporter>,
}

impl Inlet for ToRemux {
    fn tag(&mut self, tag: MediaTag) {
        if !self.seen_keyframe {
            // The AVC sequence header arrives marked as a keyframe, and it is
            // what a decoder needs first.
            if !(tag.kind == TagKind::Video && tag.keyframe) {
                return;
            }
            self.seen_keyframe = true;
        }
        let bytes = flv::write(&tag);
        self.remux.write(&bytes);
        self.state.wrote(bytes.len(), &self.remux);
    }
}

impl Drop for ToRemux {
    fn drop(&mut self) {
        self.state.set_publisher(None);
        self.busy.store(false, Ordering::Release);
        if let Some(r) = &self.reporter {
            r.info(format!("{} stopped publishing", self.name));
            r.health_changed(Health::degraded(format!(
                "{} stopped publishing. The port is still open, so the same encoder \
                 reconnecting is picked up without anything being rebuilt.",
                self.name
            )));
        }
    }
}
