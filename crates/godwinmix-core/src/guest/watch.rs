//! A thread per session that follows its peer connection: live once it
//! connects, and ended once the guest has gone for good.
//!
//! `disconnected` is waited out for fifteen seconds, longer than the
//! publisher page's five, because a short Wi-Fi blip comes back by itself
//! and the page never offers again while it might. `failed` and `closed`
//! end the session at once. A session that never connects is let go after
//! twenty seconds.

use super::Link;
use gstreamer as gst;
use gstreamer::prelude::*;
use gstreamer_webrtc::WebRTCPeerConnectionState as Peer;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

const TICK: Duration = Duration::from_millis(250);
const CONNECT_WITHIN: Duration = Duration::from_secs(20);
const DISCONNECTED_FOR: Duration = Duration::from_secs(15);

pub fn watch(id: &str, session: &str, bin: gst::Element, stop: Arc<AtomicBool>) {
    let (id, session) = (id.to_string(), session.to_string());
    let weak = bin.downgrade();
    drop(bin);
    let spawned = std::thread::Builder::new().name("gmx-guest-watch".into()).spawn(move || {
        let started = Instant::now();
        let mut away: Option<Instant> = None;
        let mut was_live = false;
        while !stop.load(Ordering::Relaxed) {
            std::thread::sleep(TICK);
            let Some(bin) = weak.upgrade() else { return };
            let state = bin.property::<Peer>("connection-state");
            drop(bin);
            if stop.load(Ordering::Relaxed) {
                return;
            }
            if state == Peer::Connected && !was_live {
                was_live = true;
                super::set_link(&id, Link::Live);
            }
            away = match state {
                Peer::Disconnected => away.or(Some(Instant::now())),
                _ => None,
            };
            let never = !was_live && started.elapsed() > CONNECT_WITHIN;
            let lost = away.is_some_and(|t| t.elapsed() > DISCONNECTED_FOR);
            if never || lost || matches!(state, Peer::Failed | Peer::Closed) {
                super::lost(&id, &session);
                return;
            }
        }
    });
    if let Err(e) = spawned {
        tracing::error!(error = %e, "could not start the thread that follows a guest's connection");
    }
}
