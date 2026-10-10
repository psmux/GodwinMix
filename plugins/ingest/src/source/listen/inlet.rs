//! The publisher's side of `ingest/rtmp` on its own port: its tags into the
//! remuxer, and when it last sent any.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use godwinmix_sdk::wire::Health;

use super::Inner;
use crate::flv;
use crate::media_tag::{MediaTag, TagKind};
use crate::rtmp::Inlet;

/// When a publisher last sent anything, readable from another thread.
pub struct Quiet {
    since: Instant,
    last_ms: AtomicU64,
}

impl Quiet {
    pub fn new() -> Quiet {
        Quiet { since: Instant::now(), last_ms: AtomicU64::new(0) }
    }

    fn touch(&self) {
        self.last_ms.store(self.since.elapsed().as_millis() as u64, Ordering::Relaxed);
    }

    /// How long since anything came, or since the publisher arrived.
    pub fn quiet(&self) -> Duration {
        self.since.elapsed().saturating_sub(Duration::from_millis(self.last_ms.load(Ordering::Relaxed)))
    }
}

/// The one publisher's tags, written on as FLV.
pub struct ToRemux {
    pub id: u64,
    pub name: String,
    /// Nothing is written until the first keyframe, the FLV header with it,
    /// so a decoder is never handed a run of inter frames with nothing to
    /// decode them against.
    pub wrote: bool,
    pub quiet: Arc<Quiet>,
    pub gate: Arc<Inner>,
}

impl Inlet for ToRemux {
    fn tag(&mut self, tag: MediaTag) {
        self.quiet.touch();
        let g = &self.gate;
        if !self.wrote {
            // The AVC sequence header arrives marked as a keyframe, and it is
            // what a decoder needs first. Only one publisher ever writes.
            if !(tag.kind == TagKind::Video && tag.keyframe) || g.spent.swap(true, Ordering::AcqRel) {
                return;
            }
            self.wrote = true;
            let header = flv::header();
            g.remux.write(&header);
            g.state.wrote(header.len(), &g.remux);
        }
        let bytes = flv::write(&tag);
        g.remux.write(&bytes);
        g.state.wrote(bytes.len(), &g.remux);
    }
}

impl Drop for ToRemux {
    fn drop(&mut self) {
        let g = &self.gate;
        let mine = {
            let mut held = g.held();
            let mine = held.as_ref().is_some_and(|h| h.id == self.id);
            if mine {
                *held = None;
            }
            mine
        };
        if mine {
            g.state.set_publisher(None);
        }
        if let Some(r) = &g.reporter {
            r.info(format!("{} stopped publishing", self.name));
        }
        if self.wrote {
            if let Some(r) = &g.reporter {
                r.info("ending this source's stream so the next publisher starts on a clean one".to_string());
            }
            (g.end)(&g.remux);
            return;
        }
        g.health(Health::degraded(format!(
            "{} stopped publishing. The port is still open, so the same encoder reconnecting is \
             picked up without anything being rebuilt.",
            self.name
        )));
    }
}
