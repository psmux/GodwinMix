//! One sending thread: a set of feeds, each a clip looping on its own
//! address, every datagram sent when its PCR says, with continuity counters
//! carried on and PCR, PTS and DTS moved on by a loop each time round, so a
//! receiver sees one stream that never restarts.

use super::clip::{Clip, PER_DATAGRAM};
use crate::ts::{self, pes, PACKET};
use std::net::{SocketAddrV4, UdpSocket};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering::Relaxed};
use std::sync::Arc;
use std::time::{Duration, Instant};

pub struct Feed {
    pub clip: Arc<Clip>,
    pub to: SocketAddrV4,
    /// Percent of datagrams dropped before the socket.
    pub loss: f64,
    /// Up to this much later than its time each datagram leaves, order kept.
    pub jitter_ns: u64,
    /// The datagram the feed starts on, so keyframes do not all line up.
    pub start: usize,
    pub seed: u64,
}

/// Set by SIGINT or SIGTERM: every thread finishes its tick and stops, so the
/// summary is still written.
pub static STOP: AtomicBool = AtomicBool::new(false);

#[derive(Default)]
pub struct Totals {
    pub datagrams: AtomicU64,
    pub bytes: AtomicU64,
    pub dropped: AtomicU64,
    pub errors: AtomicU64,
    pub late_max_us: AtomicU64,
}

/// A small xorshift: loss and jitter only have to look random.
pub struct Dice(u64);

impl Dice {
    pub fn new(seed: u64) -> Dice {
        Dice(seed | 1)
    }

    pub fn unit(&mut self) -> f64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 >> 11) as f64 / (1u64 << 53) as f64
    }
}

pub struct Live {
    feed: Feed,
    next: usize,
    round: u64,
    cc: Vec<u8>,
    due: u64,
    dice: Dice,
}

impl Live {
    pub fn new(feed: Feed) -> Live {
        let cc = vec![0; feed.clip.pids.len()];
        let (next, dice) = (feed.start % feed.clip.datagrams(), Dice::new(feed.seed));
        Live { feed, next, round: 0, cc, due: 0, dice }
    }

    /// When the current datagram is due, in nanoseconds from the feed's start.
    fn scheduled(&self) -> u64 {
        let c = &self.feed.clip;
        self.round * c.loop_ns + c.at_ns[self.next] - c.at_ns[self.feed.start % c.datagrams()]
    }

    /// The current datagram, rewritten for this time round, into `buf`.
    pub fn fill(&mut self, buf: &mut [u8]) -> usize {
        let clip = Arc::clone(&self.feed.clip);
        let first = self.next * PER_DATAGRAM;
        let last = (first + PER_DATAGRAM).min(clip.packets());
        let len = (last - first) * PACKET;
        buf[..len].copy_from_slice(&clip.data[first * PACKET..last * PACKET]);
        let shift_27m = self.round * clip.loop_27m;
        for (k, p) in buf[..len].chunks_exact_mut(PACKET).enumerate() {
            if ts::pid(p) == ts::NULL_PID {
                continue;
            }
            let s = usize::from(clip.slot[first + k]);
            if ts::has_payload(p) {
                ts::set_cc(p, self.cc[s]);
                self.cc[s] = (self.cc[s] + 1) & 0x0F;
            } else {
                ts::set_cc(p, self.cc[s].wrapping_sub(1));
            }
            if shift_27m > 0 {
                if let Some(v) = ts::pcr(p) {
                    ts::set_pcr(p, v + shift_27m);
                }
                pes::shift(p, (shift_27m / 300) % pes::PTS_WRAP);
            }
        }
        self.next += 1;
        if self.next == clip.datagrams() {
            self.next = 0;
            self.round += 1;
        }
        len
    }

    /// Sends every datagram due by `now`, answering how late the oldest of them was.
    pub fn send_due(&mut self, now: u64, sock: &UdpSocket, buf: &mut [u8], t: &Totals) -> u64 {
        let mut late = 0;
        while self.due <= now {
            let len = self.fill(buf);
            if self.dice.unit() * 100.0 < self.feed.loss {
                t.dropped.fetch_add(1, Relaxed);
            } else if sock.send_to(&buf[..len], self.feed.to).is_ok() {
                t.datagrams.fetch_add(1, Relaxed);
                t.bytes.fetch_add(len as u64, Relaxed);
            } else {
                t.errors.fetch_add(1, Relaxed);
            }
            late = late.max(now - self.due);
            let jitter = (self.dice.unit() * self.feed.jitter_ns as f64) as u64;
            self.due = self.due.max(self.scheduled() + jitter);
        }
        late
    }
}

/// Runs `feeds` on one socket until `until`, waking every `tick`.
pub fn run(feeds: Vec<Feed>, sock: UdpSocket, start: Instant, until: Instant, tick: Duration, t: &Totals) {
    let mut lives: Vec<Live> = feeds.into_iter().map(Live::new).collect();
    let mut buf = [0u8; PER_DATAGRAM * PACKET];
    let settle = start + Duration::from_secs(1);
    loop {
        let now_at = Instant::now();
        if now_at >= until || STOP.load(Relaxed) {
            return;
        }
        let now = (now_at - start).as_nanos() as u64;
        let late = lives.iter_mut().map(|l| l.send_due(now, &sock, &mut buf, t)).max().unwrap_or(0);
        if now_at > settle {
            t.late_max_us.fetch_max(late / 1000, Relaxed);
        }
        std::thread::sleep(tick);
    }
}
