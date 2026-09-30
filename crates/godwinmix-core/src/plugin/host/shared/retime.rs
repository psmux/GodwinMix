//! Keeping a shared stream's sound and pictures together.
//!
//! A reader stamps a picture from a camera when it arrives, which is all a
//! picture alone needs. Sound and pictures together cannot be stamped that
//! way: the owner decodes the picture more slowly than the sound, so the two
//! arrive apart by however long the decoder takes, and stamping on arrival
//! would put that gap into the programme as a lip sync error.
//!
//! So both tracks keep the owner's timestamps, which the owner's demuxer made
//! for both from one clock, and this moves them onto this pipeline's running
//! time with one shift shared by the two: set when the first buffer of either
//! arrives, and set again when a new owner starts a new timeline (its frame
//! numbers start again at 1), in a way that carries on from where the last
//! one left off. The later track then arrives a little behind its time, by
//! the decoder's delay, which is what a source that decodes for itself does
//! too, and the programme's queues absorb it the same way.

use gstreamer as gst;
use gstreamer::prelude::*;
use parking_lot::Mutex;
use std::sync::Arc;

/// The shift from the owner's timeline to this pipeline's running time.
#[derive(Default)]
pub struct Anchor {
    inner: Mutex<Inner>,
}

#[derive(Default)]
struct Inner {
    /// Bumped each time a new owner's timeline is anchored.
    epoch: u64,
    /// `running = owner_pts + shift`.
    shift: Option<i128>,
}

/// One track's view: the epoch it is on and the last frame number it saw.
struct Track {
    epoch: u64,
    last_seq: u64,
}

impl Anchor {
    /// Where a buffer with the owner's `pts` goes on this pipeline's running
    /// time, `now` being the running time it arrived at. `restarted` says this
    /// track just saw a new owner's first frames.
    fn place(&self, track: &mut Track, pts: u64, now: u64, restarted: bool) -> u64 {
        let mut a = self.inner.lock();
        let fresh = a.shift.is_none() || (restarted && track.epoch == a.epoch);
        if fresh {
            a.epoch += 1;
            a.shift = Some(i128::from(now) - i128::from(pts));
        }
        track.epoch = a.epoch;
        let at = i128::from(pts) + a.shift.unwrap_or(0);
        at.clamp(0, i128::from(u64::MAX)) as u64
    }

    /// Put the owner's timestamps on this pipeline's timeline, on `pad`.
    pub fn install(self: &Arc<Self>, pad: &gst::Pad) {
        let anchor = self.clone();
        let track = Mutex::new(Track { epoch: 0, last_seq: 0 });
        pad.add_probe(gst::PadProbeType::BUFFER, move |pad, info| {
            let Some(now) = running_time(pad) else { return gst::PadProbeReturn::Ok };
            let Some(buffer) = info.buffer_mut() else { return gst::PadProbeReturn::Ok };
            let Some(pts) = buffer.pts() else { return gst::PadProbeReturn::Ok };
            let mut t = track.lock();
            let seq = buffer.offset();
            let restarted = seq < t.last_seq;
            t.last_seq = seq;
            let at = anchor.place(&mut t, pts.nseconds(), now, restarted);
            buffer.make_mut().set_pts(gst::ClockTime::from_nseconds(at));
            gst::PadProbeReturn::Ok
        });
    }
}

/// The running time of the pipeline `pad` is in, now.
fn running_time(pad: &gst::Pad) -> Option<u64> {
    let el = pad.parent_element()?;
    let now = el.clock()?.time();
    Some(now.checked_sub(el.base_time()?)?.nseconds())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn track() -> Track {
        Track { epoch: 0, last_seq: 0 }
    }

    #[test]
    fn both_tracks_share_one_shift_and_keep_their_distance() {
        let a = Anchor::default();
        let (mut sound, mut picture) = (track(), track());
        // Sound at owner time 10 s arrives at running time 2 s.
        assert_eq!(a.place(&mut sound, 10_000_000_000, 2_000_000_000, false), 2_000_000_000);
        // The picture for the same moment arrives 40 ms later and keeps its time.
        assert_eq!(a.place(&mut picture, 10_000_000_000, 2_040_000_000, false), 2_000_000_000);
        assert_eq!(a.place(&mut sound, 10_020_000_000, 2_025_000_000, false), 2_020_000_000);
    }

    #[test]
    fn a_new_owner_is_anchored_once_for_both_tracks() {
        let a = Anchor::default();
        let (mut sound, mut picture) = (track(), track());
        a.place(&mut sound, 5_000_000_000, 1_000_000_000, false);
        a.place(&mut picture, 5_000_000_000, 1_000_000_000, false);
        // The new owner's timeline starts near zero; sound sees it first.
        let s = a.place(&mut sound, 100_000_000, 3_000_000_000, true);
        assert_eq!(s, 3_000_000_000);
        // The picture's first frame from the new owner uses the same shift.
        let p = a.place(&mut picture, 100_000_000, 3_050_000_000, true);
        assert_eq!(p, 3_000_000_000, "one shift for the new owner, not two");
    }
}
