//! Where one connection's loss is counted, and the programs it found.
//!
//! A TS transport (UDP, RTP, SRT, RIST) runs the udp plugin's probe on its
//! datagrams, which counts continuity errors, RTP gaps and reads the PAT,
//! PMT and SDT. SRT and RIST also say what their own retransmission could
//! not recover, read from the element on each tick.

use std::sync::atomic::Ordering::Relaxed;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use gmx_udp::counters::{Counters, SharedCatalog};
use gmx_udp::recv::probe::Probe;
use gmx_udp::ts::filter::Filter;
use gmx_udp::ts::plan::Choice;
use gstreamer as gst;

use super::stats::{InputStats, ProgramInfo};

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
