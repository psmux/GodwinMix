//! A clip transition's clip, opened off the mixer loop.
//!
//! Opening a clip builds a decoding pipeline and starts it: the file is
//! opened, its type found, a demuxer and a decoder loaded. On a warm machine
//! that is one or two hundred milliseconds. In the seconds after a restart,
//! with every source and plugin starting at once, it was seconds, and it ran
//! inside the take, so every command queued behind the take waited and
//! `core.status` answered 503.
//!
//! So the take only asks for the clip (`open_clip`). A worker opens it and
//! `Command::FxOpened` brings the player back to the loop. The take was
//! already answered at once and already held its cut until the clip's first
//! frame (`fx_take`); what moved is where the opening happens:
//!
//! ```text
//!   take:   defer_fx -> open_clip -> worker: Player::start
//!   worker: FxOpened -> loop: the pending transition keeps the player
//!   first frame (or READY_WAIT): FxReady -> loop: cut, clip pass on
//! ```
//!
//! `FxReady` is never sent before `FxOpened`, so the loop never reaches the
//! cut thinking the clip is not open when it is. A clip that fails to open,
//! or is still opening when the wait runs out, is a cut, as a broken file
//! always was, and a player that arrives after its take has moved on is
//! stopped.

use super::{Command, Mixer, MixerHandle};
use crate::fx::player::Player;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::Arc;
use tracing::warn;

/// The clip has its first frame.
const FRAMED: u8 = 1;
/// `FxOpened` is on the queue.
const QUEUED: u8 = 2;

/// A player on its way to the loop. Stopped if nothing takes it out: a
/// player dropped without a stop keeps its pipeline parked on a full sink.
pub struct Opened(Option<Player>);

impl Opened {
    pub fn into_player(mut self) -> Option<Player> {
        self.0.take()
    }
}

impl Drop for Opened {
    fn drop(&mut self) {
        if let Some(p) = &self.0 {
            p.stop();
        }
    }
}

impl std::fmt::Debug for Opened {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Opened")
    }
}

impl Mixer {
    /// Open the clip of transition `id` on a worker and hand it back.
    pub(super) fn open_clip(&self, id: u64, path: PathBuf, size: (i32, i32)) {
        let handle = self.handle.clone();
        self.rt.spawn_blocking(move || {
            let state = Arc::new(AtomicU8::new(0));
            let opened = Player::start(&path, size).map_err(|e| format!("{e:#}")).map(|p| {
                let (tx, state) = (handle.clone(), state.clone());
                p.on_first_frame(move || {
                    if framed(&state) {
                        ready(&tx, id);
                    }
                });
                Opened(Some(p))
            });
            // A refused send drops the player, and `Opened` stops it. The
            // take's own fallback still cuts on time.
            let _ = handle.send(Command::FxOpened { transition: id, opened });
            if queued(&state) {
                ready(&handle, id);
            }
        });
    }

    /// The worker is back with the clip, or without it.
    pub(super) fn fx_opened(&mut self, id: u64, opened: Result<Opened, String>) {
        let Some(pending) = self.pending_fx.as_mut().filter(|p| p.id == id) else {
            // Overtaken by a newer take, or cut already. Dropping it stops it.
            return;
        };
        match opened {
            Ok(o) => pending.player = o.into_player(),
            Err(e) => {
                warn!(error = %e, "the clip would not open; the take is a cut");
                self.fx_ready(id);
            }
        }
    }
}

/// The first frame is here. True when the player is already queued, so this
/// side sends `FxReady`.
fn framed(state: &AtomicU8) -> bool {
    state.fetch_or(FRAMED, Ordering::SeqCst) & QUEUED != 0
}

/// `FxOpened` is queued. True when the first frame came first, so this side
/// sends `FxReady`.
fn queued(state: &AtomicU8) -> bool {
    state.fetch_or(QUEUED, Ordering::SeqCst) & FRAMED != 0
}

fn ready(handle: &MixerHandle, id: u64) {
    let _ = handle.send(Command::FxReady { transition: id });
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Whichever of the two flags lands second sends `FxReady`, and only
    /// that one: the cut is asked for once, after the player is queued.
    #[test]
    fn exactly_one_side_asks_for_the_cut() {
        let frame_first = AtomicU8::new(0);
        assert!(!framed(&frame_first));
        assert!(queued(&frame_first));
        let queued_first = AtomicU8::new(0);
        assert!(!queued(&queued_first));
        assert!(framed(&queued_first));
    }
}
