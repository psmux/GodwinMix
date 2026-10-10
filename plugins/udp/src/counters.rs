//! What the receive path counts, shared between the streaming thread that
//! counts it and the thread that reports it.
//!
//! Atomics only. The streaming thread adds, the health thread reads, and
//! neither ever waits for the other.

use std::sync::atomic::{AtomicU64, Ordering::Relaxed};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use serde_json::{json, Value};

use crate::ts::tables::Program;

#[derive(Debug)]
pub struct Counters {
    started: Instant,
    pub datagrams: AtomicU64,
    pub bytes_in: AtomicU64,
    pub bytes_out: AtomicU64,
    /// Stuffing dropped before the core sees it.
    pub nulls: AtomicU64,
    /// TS packets the continuity counters say never arrived.
    pub ts_lost: AtomicU64,
    /// Continuity counter errors: one per jump, however many packets it
    /// skipped, which is how TR 101 290 (1.4) counts them.
    pub cc_errors: AtomicU64,
    /// RTP datagrams the sequence numbers say never arrived.
    pub rtp_lost: AtomicU64,
    /// Packets without a sync byte, and datagrams that were neither TS nor RTP.
    pub malformed: AtomicU64,
    /// Packets the sender itself flagged as damaged (transport_error_indicator).
    pub flagged: AtomicU64,
    /// Times the feed went quiet and came back.
    pub resumed: AtomicU64,
    /// Times a PAT named other programs than the one before it under the
    /// same version number: a sender restarted with a new layout, which a
    /// demuxer holding the old PAT may take for the one it already read.
    pub relayouts: AtomicU64,
    /// Milliseconds after `started` that the last datagram arrived, plus one,
    /// so zero means never.
    last_ms: AtomicU64,
}

impl Default for Counters {
    fn default() -> Counters {
        Counters {
            started: Instant::now(),
            datagrams: AtomicU64::new(0),
            bytes_in: AtomicU64::new(0),
            bytes_out: AtomicU64::new(0),
            nulls: AtomicU64::new(0),
            ts_lost: AtomicU64::new(0),
            cc_errors: AtomicU64::new(0),
            rtp_lost: AtomicU64::new(0),
            malformed: AtomicU64::new(0),
            flagged: AtomicU64::new(0),
            resumed: AtomicU64::new(0),
            relayouts: AtomicU64::new(0),
            last_ms: AtomicU64::new(0),
        }
    }
}

impl Counters {
    /// A datagram arrived. Answers how long the feed had been silent before it,
    /// in milliseconds, or `None` if this is the first.
    pub fn arrived(&self, bytes: usize) -> Option<u64> {
        let now = self.started.elapsed().as_millis() as u64 + 1;
        let before = self.last_ms.swap(now, Relaxed);
        self.datagrams.fetch_add(1, Relaxed);
        self.bytes_in.fetch_add(bytes as u64, Relaxed);
        (before != 0).then(|| now - before)
    }

    /// How long since the last datagram, or `None` if none has come.
    pub fn silent_ms(&self) -> Option<u64> {
        let last = self.last_ms.load(Relaxed);
        (last != 0).then(|| (self.started.elapsed().as_millis() as u64 + 1).saturating_sub(last))
    }

    pub fn add(counter: &AtomicU64, n: u64) {
        if n > 0 {
            counter.fetch_add(n, Relaxed);
        }
    }

    /// TS packets lost, which is what health reports. An RTP datagram that
    /// went missing shows here too, as the seven packets it carried, so the
    /// RTP count is not added on top.
    pub fn lost(&self) -> u64 {
        self.ts_lost.load(Relaxed)
    }

    pub fn json(&self) -> Value {
        json!({
            "datagrams": self.datagrams.load(Relaxed),
            "bytes_in": self.bytes_in.load(Relaxed),
            "bytes_out": self.bytes_out.load(Relaxed),
            "null_packets_dropped": self.nulls.load(Relaxed),
            "ts_packets_lost": self.ts_lost.load(Relaxed),
            "cc_errors": self.cc_errors.load(Relaxed),
            "rtp_packets_lost": self.rtp_lost.load(Relaxed),
            "malformed": self.malformed.load(Relaxed),
            "flagged_by_sender": self.flagged.load(Relaxed),
            "resumed": self.resumed.load(Relaxed),
            "silent_ms": self.silent_ms(),
        })
    }
}

/// What the tables say, published by the streaming thread when it changes.
#[derive(Debug, Clone, Default)]
pub struct Catalog {
    pub programs: Vec<Program>,
    /// The program being passed to the core, once there is one.
    pub chosen: Option<u16>,
    /// Why nothing is being passed, when that is the case.
    pub problem: Option<String>,
}

impl Catalog {
    pub fn json(&self) -> Value {
        json!({
            "programs": self.programs.iter().map(Program::json).collect::<Vec<_>>(),
            "chosen": self.chosen,
            "problem": self.problem,
        })
    }

    /// "program 2 (News) of 3: 1 (Sport), 2 (News), 3 (Film)".
    pub fn phrase(&self) -> Option<String> {
        if self.programs.len() < 2 {
            return None;
        }
        let all: Vec<String> = self.programs.iter().map(Program::label).collect();
        let chosen = self.chosen.and_then(|n| self.programs.iter().find(|p| p.number == n));
        Some(match chosen {
            Some(p) => format!("program {} of {}: {}", p.label(), all.len(), all.join(", ")),
            None => format!("{} programs: {}", all.len(), all.join(", ")),
        })
    }
}

pub type SharedCatalog = Arc<Mutex<Catalog>>;
