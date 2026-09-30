//! WHEP playback: the programme, or any rendition of it, to WebRTC viewers,
//! served from the control port with no port of its own for signalling.
//!
//! A person adds a `whep/output` (it may carry a rendition, like any other
//! output), and a player POSTs its offer to `/whep/<output id>`, or to
//! `/whep/program` for the first WHEP output there is. The answer comes back
//! with every candidate in it. Media leaves on UDP ports ICE chooses, only
//! while somebody is watching.
//!
//! Nothing runs unless asked: no output, nothing built; an output with no
//! viewers costs a parser and one Opus encode, and each viewer adds a
//! packetiser and an encrypted session over the same encode. The video is
//! never encoded again for WebRTC.
//!
//! * [`output`]: the output kind, which builds the tees.
//! * [`server`]: one output's viewers, and the watcher that ends the dead.
//! * [`session`], [`negotiate`]: one viewer's webrtcbin and its answer.
//! * [`sdp`]: reading the offer's payload types.
//! * [`params`]: the output's params and each codec's payloader.
//!
//! `docs/how-to/watch-over-webrtc.md` is the person's side of it.

mod negotiate;
mod tees;
pub mod output;
pub mod params;
pub mod sdp;
pub mod server;
pub mod session;

#[cfg(test)]
mod tests;

use parking_lot::RwLock;
use server::Server;
use std::collections::BTreeMap;
use std::sync::{Arc, OnceLock};

/// A refusal, as the HTTP status to answer with and the sentence to say.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Refusal {
    pub status: u16,
    pub message: String,
}

impl Refusal {
    pub fn new(status: u16, message: impl Into<String>) -> Refusal {
        Refusal { status, message: message.into() }
    }
    pub fn bad(message: impl Into<String>) -> Refusal {
        Refusal::new(400, message)
    }
    pub fn internal(message: impl Into<String>) -> Refusal {
        Refusal::new(500, message)
    }
    pub fn unavailable(message: impl Into<String>) -> Refusal {
        Refusal::new(501, message)
    }
}

fn registry() -> &'static RwLock<BTreeMap<String, Arc<Server>>> {
    static SERVERS: OnceLock<RwLock<BTreeMap<String, Arc<Server>>>> = OnceLock::new();
    SERVERS.get_or_init(Default::default)
}

pub(crate) fn publish(server: Arc<Server>) {
    registry().write().insert(server.id.clone(), server);
}

pub(crate) fn withdraw(server: &Arc<Server>) {
    let mut map = registry().write();
    if map.get(&server.id).is_some_and(|s| Arc::ptr_eq(s, server)) {
        map.remove(&server.id);
    }
}

/// Every WHEP output this core is serving, by id.
pub fn ids() -> Vec<String> {
    registry().read().keys().cloned().collect()
}

/// The output a `/whep/<target>` means: its id, or `program` for the first.
fn resolve(target: &str) -> Result<Arc<Server>, Refusal> {
    let map = registry().read();
    if let Some(s) = map.get(target) {
        return Ok(s.clone());
    }
    if matches!(target, "program" | "programme") {
        if let Some(s) = map.values().next() {
            return Ok(s.clone());
        }
    }
    let have = if map.is_empty() { "none yet".to_string() } else { map.keys().cloned().collect::<Vec<_>>().join(", ") };
    Err(Refusal::new(404, format!(
        "no WHEP output called '{target}'. Add one in Outputs (WebRTC viewers) and POST the offer to /whep/<its id>. WHEP outputs here: {have}."
    )))
}

/// A viewer's offer: the session id and the answer. Blocks for up to three
/// seconds while ICE gathers, so call it off the async runtime.
pub fn offer(target: &str, sdp: &str) -> Result<(String, String, String), Refusal> {
    let server = resolve(target)?;
    let (session, answer) = server.offer(sdp)?;
    Ok((server.id.clone(), session, answer))
}

/// Whether `key` is the viewer key of the output `target` names. A viewer
/// with the key needs no control token, as with an HLS output.
pub fn admits(target: &str, key: &str) -> bool {
    !key.is_empty() && resolve(target).is_ok_and(|s| s.admits(key))
}

/// `DELETE` on a session. False when there was none by that id.
pub fn end(target: &str, session: &str) -> bool {
    resolve(target).map(|s| s.end(session)).unwrap_or(false)
}
