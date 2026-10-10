//! The work done on each datagram, on the streaming thread.
//!
//! Classify it (TS or RTP), count what is missing, filter it to the chosen
//! program, and say what to do with the buffer. When nothing changed, the
//! original buffer goes on and nothing is copied.

use std::sync::Arc;

use crate::counters::{Catalog, Counters, SharedCatalog};
use crate::ts::filter::Filter;
use crate::ts::plan::Plan;
use crate::ts::rtp::{classify, Datagram, Sequence};

/// Quiet for longer than this and the sender is assumed to have restarted:
/// its counters start again from wherever they like, and that is not loss.
pub const RESTART_MS: u64 = 1000;

pub enum Verdict {
    Keep,
    Drop,
    Replace(Vec<u8>),
}

pub struct Probe {
    filter: Filter,
    seq: Sequence,
    out: Vec<u8>,
    counters: Arc<Counters>,
    catalog: SharedCatalog,
}

impl Probe {
    pub fn new(filter: Filter, counters: Arc<Counters>, catalog: SharedCatalog) -> Probe {
        Probe { filter, seq: Sequence::default(), out: Vec::with_capacity(2048), counters, catalog }
    }

    pub fn datagram(&mut self, d: &[u8]) -> Verdict {
        let counters = Arc::clone(&self.counters);
        let n = &*counters;
        if n.arrived(d.len()).is_some_and(|gap| gap > RESTART_MS) {
            self.filter.resumed();
            self.seq.reset();
            Counters::add(&n.resumed, 1);
        }
        let packets = match classify(d) {
            Datagram::Ts(p) => p,
            Datagram::Rtp { seq, payload } => {
                Counters::add(&n.rtp_lost, self.seq.lost_before(seq));
                payload
            }
            Datagram::Junk => {
                Counters::add(&n.malformed, 1);
                return Verdict::Drop;
            }
        };
        self.out.clear();
        let modified = self.filter.feed(packets, n, &mut self.out);
        if std::mem::take(&mut self.filter.relayout) {
            Counters::add(&n.relayouts, 1);
        }
        self.publish();
        Counters::add(&n.bytes_out, self.out.len() as u64);
        if self.out.is_empty() {
            Verdict::Drop
        } else if !modified && packets.len() == d.len() {
            Verdict::Keep
        } else {
            Verdict::Replace(std::mem::replace(&mut self.out, Vec::with_capacity(2048)))
        }
    }

    /// Copy the tables out for the health thread, when they changed and the
    /// lock is free. Busy means try again on the next datagram, never wait.
    fn publish(&mut self) {
        if !self.filter.changed {
            return;
        }
        let Ok(mut c) = self.catalog.try_lock() else { return };
        *c = catalog_of(&self.filter);
        self.filter.changed = false;
    }
}

fn catalog_of(f: &Filter) -> Catalog {
    let (chosen, problem) = match f.plan() {
        Plan::Only(s) => (Some(s.program), None),
        Plan::PassAll => (f.programs().first().map(|p| p.number), None),
        Plan::Missing(why) => (None, Some(why.clone())),
        Plan::Waiting => (None, None),
    };
    Catalog { programs: f.programs().to_vec(), chosen, problem }
}
