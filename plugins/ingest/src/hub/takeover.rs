//! A publisher that went away without hanging up, replaced by the one that
//! came back.
//!
//! A browser tab closed, a browser killed or a laptop put to sleep sends no
//! WHIP `DELETE` and no RTCP BYE, and a dropped network sends an RTMP or SRT
//! encoder no FIN. Its session stays in the hub until the protocol's own
//! timeout ends it: about 30 seconds for WebRTC's ICE, longer for a TCP
//! connection nobody writes to. Every new publish to that name in between was
//! refused as "already being published", so a page reload stayed off air for
//! half a minute.
//!
//! So a session that has sent nothing for [`STALE`] may be taken over by a
//! new publisher the gate let in, which already means it holds a valid key.
//! The old session is cut off with its own kick, its readers are told it
//! ended (a reader never splices two publishers' timelines), and the new one
//! starts as any other would. A session that is still sending is never taken
//! over, so a second tab genuinely live on the same name is refused as it
//! always was.
//!
//! Only a session with a kick can be taken over: one a publisher opened
//! through the gate. The hub's own publishers (a transcode's output, a direct
//! show's input) have none and are refused as before.

use std::time::Duration;

use super::{lock, meter, Hub, Publication, Session, SlotState};
use crate::rtmp::Kick;

/// How long a session must have sent nothing before a new publisher may take
/// its name. Every working publisher sends far more often than this: video at
/// its frame rate (a static screen share still sends a frame a second) and
/// sound every 20 to 40 ms. It is also the mark at which the mixer itself
/// judges a source stalled, so a stream this quiet is already off the
/// programme. A page that reloads faster than this is not refused: it waits
/// out the rest, see [`HESITATE`].
pub const STALE: Duration = Duration::from_secs(2);

/// A session quiet for this long is not a publisher that is working: one
/// sends sound every 20 to 40 ms and a picture every frame. A newcomer that
/// finds a session this quiet waits for it to reach [`STALE`] rather than
/// being turned away, so a page that reloads in under two seconds is not
/// refused for arriving early. Below this the newcomer is refused at once.
pub const HESITATE: Duration = Duration::from_millis(500);

/// What a newcomer finds on a name.
enum Found {
    Free,
    /// Quiet for [`STALE`]: take it.
    Quiet,
    /// Quiet for [`HESITATE`]: wait this long and look again.
    Wait(Duration),
    /// Live, or not ours to take: refused with this sentence.
    Held(String),
}

impl Hub {
    /// Start a session that arrived over `via`, with the means to cut it off.
    /// A name held by a session that has gone quiet for [`STALE`] is taken
    /// over; one that is still sending is refused. May wait up to [`STALE`]
    /// for a session that is going quiet to finish doing so, so this is only
    /// called from a publisher's own thread.
    pub fn publish_with(
        &self,
        app: &str,
        stream: &str,
        from: &str,
        key: Option<String>,
        via: &'static str,
        kick: Option<Kick>,
    ) -> Result<Publication, String> {
        loop {
            let mut slots = lock(&self.inner.slots);
            let slot = Hub::slot(&mut slots, app, stream);
            let mut state = lock(&slot.state);
            drop(slots);
            let gone = match found(&state, app, stream) {
                Found::Held(why) => return Err(why),
                Found::Wait(wait) => {
                    drop(state);
                    std::thread::sleep(wait.min(Duration::from_millis(100)));
                    continue;
                }
                Found::Quiet => take(&mut state),
                Found::Free => None,
            };
            let id = self.inner.sessions.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
            let meter = meter::Meter::new();
            state.session = Some(Session { id, from: from.to_string(), key, via, meter, kick });
            state.headers = Default::default();
            state.dropped_gops = 0;
            drop(state);
            // Outside every lock: a kick closes a socket or stops a pipeline.
            let took_over = gone.map(|(from, kick)| {
                kick();
                from
            });
            return Ok(Publication { inner: self.inner.clone(), slot, id, took_over });
        }
    }

    /// Whether a new publisher to this name would be refused at once: a
    /// session holds it and is still sending, or cannot be taken over at all.
    pub fn held(&self, app: &str, stream: &str) -> bool {
        let slot = lock(&self.inner.slots).get(&(app.to_string(), stream.to_string())).cloned();
        slot.is_some_and(|s| matches!(found(&lock(&s.state), app, stream), Found::Held(_)))
    }
}

fn found(state: &SlotState, app: &str, stream: &str) -> Found {
    let Some(live) = &state.session else { return Found::Free };
    let quiet = live.meter.quiet();
    match live.kick.is_some() {
        true if quiet >= STALE => Found::Quiet,
        true if quiet >= HESITATE => Found::Wait(STALE - quiet),
        _ => Found::Held(format!(
            "{app}/{stream} is already being published from {}. Give this encoder \
             another stream name, or stop the other one first.",
            live.from
        )),
    }
}

/// End the quiet session in the slot: every reader told, and its kick handed
/// back to be called once the lock is let go.
fn take(state: &mut SlotState) -> Option<(String, Kick)> {
    let old = state.session.take()?;
    for reader in state.readers.drain(..) {
        reader.end();
    }
    old.kick.map(|kick| (old.from, kick))
}

#[cfg(test)]
#[path = "takeover_tests.rs"]
mod tests;
