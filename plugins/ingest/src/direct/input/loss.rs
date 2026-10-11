//! Where one connection's loss is counted, and the programs it found.
//!
//! A TS transport (UDP, RTP, SRT, RIST) runs the udp plugin's probe on its
//! datagrams, which counts continuity errors, RTP gaps and reads the PAT,
//! PMT and SDT. SRT and RIST also say what their own retransmission could
//! not recover, read from the element on each tick.

use std::sync::atomic::Ordering::Relaxed;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use gmx_udp::counters::{Counters, SharedCatalog};
use gmx_udp::recv::probe::Probe;
use gmx_udp::ts::filter::Filter;
use gmx_udp::ts::plan::Choice;
use gstreamer as gst;

use super::stats::{InputStats, ProgramInfo};

/// A datagram within this long, and the sender is still sending.
const SENDING_MS: u64 = 1_000;
/// How long a session may take datagrams it reads nothing from, after the
/// feed has been read before, until it is built again. See `Loss::unread`.
pub const UNREAD_MS: u64 = 5_000;

pub struct Loss {
    ts: Option<(Arc<Counters>, SharedCatalog)>,
    /// Packets the transport itself lost, read from its element.
    poll: Option<Box<dyn Fn() -> u64 + Send>>,
    /// The probe's byte count and when it was read, for the rate.
    rate: Mutex<(u64, Instant)>,
}

impl Default for Loss {
    fn default() -> Loss {
        Loss { ts: None, poll: None, rate: Mutex::new((0, Instant::now())) }
    }
}

impl Loss {
    /// Nothing is counted: a file, or a transport that hides its loss.
    pub fn none() -> Loss {
        Loss::default()
    }

    /// Put the TS probe on `element`'s src pad, keeping `program` (or the
    /// first), as the udp plugin runs it on `udpsrc`.
    pub fn probe(element: &gst::Element, program: Option<u16>) -> Result<Loss, String> {
        let counters = Arc::new(Counters::default());
        let catalog = SharedCatalog::default();
        let choice = Choice { program: program.unwrap_or(0), pids: Vec::new() };
        let probe = Probe::new(Filter::new(choice), counters.clone(), catalog.clone());
        gmx_udp::recv::elements::attach_probe(element, probe)?;
        Ok(Loss { ts: Some((counters, catalog)), poll: None, rate: Mutex::new((0, Instant::now())) })
    }

    /// Also read the transport's own loss with `poll`.
    pub fn with_poll(mut self, poll: impl Fn() -> u64 + Send + 'static) -> Loss {
        self.poll = Some(Box::new(poll));
        self
    }

    /// `(cc_errors, packets_lost)` this connection, so far.
    pub fn now(&self) -> (u64, u64) {
        let polled = self.poll.as_ref().map(|p| p());
        let Some((n, _)) = &self.ts else { return (0, polled.unwrap_or(0)) };
        let (rtp, ts) = (n.rtp_lost.load(Relaxed), n.ts_lost.load(Relaxed));
        let lost = polled.unwrap_or(if rtp > 0 { rtp } else { ts });
        (n.cc_errors.load(Relaxed), lost)
    }

    /// Write the counts, added to `before` from earlier connections, and the
    /// programs into `s`. The rate is the chosen program's, without stuffing.
    pub fn fill(&self, s: &mut InputStats, before: (u64, u64)) {
        let (cc, lost) = self.now();
        (s.cc_errors, s.packets_lost) = (before.0 + cc, before.1 + lost);
        let Some((n, catalog)) = &self.ts else { return };
        if let Some(kbps) = self.kbps(n) {
            s.kbps = kbps;
        }
        let Ok(c) = catalog.try_lock() else { return };
        s.program = c.chosen.or(s.program);
        if !c.programs.is_empty() {
            s.programs = c
                .programs
                .iter()
                .map(|p| ProgramInfo {
                    number: p.number,
                    name: p.name.clone(),
                    provider: p.provider.clone(),
                    streams: p.streams.iter().map(|s| s.kind().to_string()).collect(),
                })
                .collect();
        }
    }

    /// Why this session should be built again, if it should: frames came out
    /// of it before, the sender is still sending, and for [`UNREAD_MS`] of
    /// this session nothing readable has come out. `quiet` is the time since
    /// the last frame on any session, `session` how long this one has run.
    ///
    /// A UDP socket is never reconnected for silence, since it stays open
    /// whether anyone sends or not. But a sender started again with new PIDs
    /// and a new program number kept arriving at it while the demuxer went
    /// on waiting for the old program: on a macOS runner the input read
    /// "nothing for 35 s" with 2.4 Mbit/s coming in. A fresh demuxer reads
    /// the new tables at once. Only a feed that was read before is judged,
    /// so one that never carried anything readable is not rebuilt for ever.
    pub fn unread(&self, quiet: Option<u64>, session: Duration, address: &str) -> Option<String> {
        let (n, _) = self.ts.as_ref()?;
        let unread = quiet?.min(session.as_millis() as u64);
        let sending = n.silent_ms().is_some_and(|s| s < SENDING_MS);
        (sending && unread > UNREAD_MS).then(|| {
            format!("{address} is sending, but nothing in it could be read for {} s; reading it again from the start", unread / 1000)
        })
    }

    /// The rate of what the probe passed on since the last call: the chosen
    /// program, stuffing left out.
    fn kbps(&self, n: &Counters) -> Option<u32> {
        let mut last = self.rate.lock().unwrap_or_else(|e| e.into_inner());
        let (bytes, secs) = (n.bytes_out.load(Relaxed), last.1.elapsed().as_secs_f64());
        if secs < 0.5 {
            return None;
        }
        let kbps = (bytes.saturating_sub(last.0) as f64 * 8.0 / 1000.0 / secs).round() as u32;
        *last = (bytes, Instant::now());
        Some(kbps)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A session whose sender sent a datagram just now.
    fn sending() -> Loss {
        let n = Arc::new(Counters::default());
        n.arrived(188);
        Loss { ts: Some((n, SharedCatalog::default())), ..Loss::default() }
    }

    #[test]
    fn a_feed_read_before_and_still_sending_is_read_again_after_five_seconds() {
        let l = sending();
        let why = l.unread(Some(6_000), Duration::from_secs(10), "udp://127.0.0.1:20201");
        assert!(why.is_some_and(|w| w.contains("is sending") && w.contains("udp://127.0.0.1:20201")));
        assert!(l.unread(Some(6_000), Duration::from_secs(3), "u").is_none(), "a new session gets its own five seconds");
        assert!(l.unread(Some(2_000), Duration::from_secs(10), "u").is_none(), "frames came two seconds ago");
        assert!(l.unread(None, Duration::from_secs(60), "u").is_none(), "a feed never read is never rebuilt for it");
        assert!(Loss::none().unread(Some(60_000), Duration::from_secs(60), "u").is_none(), "no probe, nothing to judge");
    }

    #[test]
    fn a_sender_that_stopped_is_waited_for_not_rebuilt() {
        let l = Loss { ts: Some((Arc::new(Counters::default()), SharedCatalog::default())), ..Loss::default() };
        assert!(l.unread(Some(60_000), Duration::from_secs(60), "u").is_none(), "nothing is arriving");
    }
}
