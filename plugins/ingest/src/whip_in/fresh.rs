//! A publisher that comes back to `ingest/whip` starts a fresh stream.
//!
//! `matroskamux streamable=true` writes its track list with its first
//! buffer and takes no new stream after that, and what it writes down
//! stdout is one stream with one timeline. A second publisher's pads,
//! linked into the same muxer, carried nothing: the source sat on its
//! freeze frame until somebody restarted it. So once a publisher's media
//! has gone through the muxer, that publisher leaving (its pads removed)
//! or a new one arriving (a pad added well after the first media) ends
//! this process. The core restarts it in place, behind its freeze frame,
//! and the new process takes the next offer on a clean pipe. A publishing
//! page offers again by itself until it is taken.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use godwinmix_sdk::plugin::Reporter;
use gstreamer as gst;
use gstreamer::prelude::*;

/// A pad added this long after the first media is a new publisher, not
/// the first one's second stream, which arrives within milliseconds.
pub const LATE: Duration = Duration::from_secs(2);

pub struct Fresh {
    started: Instant,
    /// Milliseconds after `started` that media first went through the
    /// muxer, plus one; 0 while nothing has.
    flowed: AtomicU64,
    spent: AtomicBool,
    exit: bool,
    reporter: Option<Reporter>,
}

impl Fresh {
    /// `exit`: end the process when the stream is spent. A test writing to
    /// a file says no, and reads [`Fresh::spent`] instead.
    pub fn new(exit: bool, reporter: Option<Reporter>) -> Arc<Fresh> {
        Arc::new(Fresh { started: Instant::now(), flowed: AtomicU64::new(0), spent: AtomicBool::new(false), exit, reporter })
    }

    /// Note the first buffer out of the muxer. A probe that sets a flag,
    /// on the streaming thread, and then takes itself off.
    pub fn watch(self: &Arc<Self>, mux: &gst::Element) {
        let Some(pad) = mux.static_pad("src") else { return };
        let me = Arc::clone(self);
        pad.add_probe(gst::PadProbeType::BUFFER, move |_, _| {
            me.flowed();
            gst::PadProbeReturn::Remove
        });
    }

    pub fn flowed(&self) {
        let at = self.started.elapsed().as_millis() as u64 + 1;
        let _ = self.flowed.compare_exchange(0, at, Ordering::AcqRel, Ordering::Acquire);
    }

    /// A pad was added. False when it belongs to a publisher this stream
    /// cannot carry, which ends it.
    pub fn pad_added(&self) -> bool {
        let at = self.flowed.load(Ordering::Acquire);
        let late = at > 0 && self.started.elapsed() > Duration::from_millis(at - 1) + LATE;
        if late {
            self.end("a new publisher arrived");
        }
        !late && !self.spent()
    }

    /// A pad went: its publisher has left.
    pub fn pad_removed(&self) {
        if self.flowed.load(Ordering::Acquire) > 0 {
            self.end("the publisher left");
        }
    }

    pub fn spent(&self) -> bool {
        self.spent.load(Ordering::Acquire)
    }

    /// Once: say so, and in a real source exit from a thread of its own, so
    /// no streaming thread waits on it.
    fn end(&self, why: &str) {
        if self.spent.swap(true, Ordering::AcqRel) {
            return;
        }
        if let Some(r) = &self.reporter {
            r.info(format!("{why}; ending this source's stream so the next publisher starts on a clean one"));
        }
        if self.exit {
            let _ = std::thread::Builder::new().name("gmx-whip-fresh".into()).spawn(|| std::process::exit(0));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_first_publishers_second_stream_joins_and_a_later_publisher_ends_the_stream() {
        let fresh = Fresh::new(false, None);
        assert!(fresh.pad_added(), "video, before anything flowed");
        fresh.flowed();
        assert!(fresh.pad_added(), "audio, milliseconds later, is the same publisher");
        assert!(!fresh.spent());
        std::thread::sleep(LATE + Duration::from_millis(100));
        assert!(!fresh.pad_added(), "a pad this late is somebody new");
        assert!(fresh.spent());
    }

    #[test]
    fn a_publisher_leaving_after_its_media_flowed_ends_the_stream_and_one_that_sent_nothing_does_not() {
        let quiet = Fresh::new(false, None);
        quiet.pad_removed();
        assert!(!quiet.spent(), "nothing reached the muxer, so the next publisher can use it");
        let used = Fresh::new(false, None);
        used.flowed();
        used.pad_removed();
        assert!(used.spent());
    }
}
