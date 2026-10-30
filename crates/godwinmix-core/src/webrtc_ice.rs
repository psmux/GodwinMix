//! Whether a `webrtcbin` gathers ICE-TCP candidates.
//!
//! ICE-TCP has the agent listen on a TCP port on every network address the
//! machine has, so a viewer or a guest on a network that blocks UDP can
//! still reach the mixer. The installed app keeps it on, and its installer
//! writes the firewall rules for it. A test talks to itself on one machine,
//! where those listeners do nothing but ask Windows to allow each new test
//! program through the firewall, so the WebRTC tests turn it off.
//! `GODWINMIX_ICE_TCP=0` turns it off for a whole process.

use gstreamer as gst;
use gstreamer::prelude::*;
use std::sync::atomic::{AtomicU8, Ordering};

const UNSET: u8 = 0;
const ON: u8 = 1;
const OFF: u8 = 2;

static ICE_TCP: AtomicU8 = AtomicU8::new(UNSET);

/// Turn ICE-TCP on or off for every `webrtcbin` this process makes from now.
pub fn set_tcp(on: bool) {
    ICE_TCP.store(if on { ON } else { OFF }, Ordering::Relaxed);
}

fn tcp() -> bool {
    match ICE_TCP.load(Ordering::Relaxed) {
        ON => true,
        OFF => false,
        _ => !matches!(std::env::var("GODWINMIX_ICE_TCP").as_deref(), Ok("0" | "off" | "false")),
    }
}

/// Apply the setting to a `webrtcbin`, before it gathers anything.
pub fn apply(webrtcbin: &gst::Element) {
    if tcp() || webrtcbin.find_property("ice-agent").is_none() {
        return;
    }
    let agent = webrtcbin.property::<glib::Object>("ice-agent");
    if agent.find_property("ice-tcp").is_some() {
        agent.set_property("ice-tcp", false);
    }
}
