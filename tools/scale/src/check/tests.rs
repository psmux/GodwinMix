//! The sender and the checker against each other: in memory across many
//! loops, and over real loopback sockets.

use super::stream::Stream;
use crate::feeds::clip::{tests::synthetic, Clip};
use crate::feeds::send::{self, Feed, Live, Totals};
use crate::net;
use std::sync::Arc;
use std::time::{Duration, Instant};

fn feed(clip: &Arc<Clip>, to: &str, loss: f64) -> Feed {
    Feed { clip: Arc::clone(clip), to: to.parse().unwrap(), loss, jitter_ns: 0, start: 37, seed: 7 }
}

/// Every datagram of `loops` passes, in order, as a receiver would see them,
/// timed by their schedule.
fn through(clip: &Arc<Clip>, loops: usize, skip: Option<usize>) -> Stream {
    let mut live = Live::new(feed(clip, "127.0.0.1:9", 0.0));
    let mut s = Stream::new();
    let mut buf = [0u8; 1316];
    for i in 0..loops * clip.datagrams() {
        let n = live.fill(&mut buf);
        if Some(i) != skip {
            s.datagram(&buf[..n], i as f64 * clip.loop_ns as f64 / clip.datagrams() as f64 / 1e6);
        }
    }
    s
}

#[test]
fn looping_is_seamless_for_continuity_and_pcr() {
    let clip = Arc::new(Clip::from_bytes("t.ts".into(), synthetic(700, 2700)).unwrap());
    let s = through(&clip, 5, None);
    assert_eq!(s.cc_errors, 0);
    assert_eq!(s.pcr_jumps, 0, "the PCR runs on across every loop");
    // A PCR every millisecond, two across the loop where the PAT and PMT sit.
    assert!(s.pcr_gap_max_ms > 0.0 && s.pcr_gap_max_ms <= 2.0 + 1e-9, "{}", s.pcr_gap_max_ms);
    // Seven packets share one arrival time, so a datagram (0.7 ms here) is the floor.
    assert!(s.jitter_ms() < 0.7, "{}", s.jitter_ms());
}

#[test]
fn a_lost_datagram_is_a_continuity_error() {
    let clip = Arc::new(Clip::from_bytes("t.ts".into(), synthetic(700, 2700)).unwrap());
    let s = through(&clip, 2, Some(150));
    assert_eq!(s.cc_errors, 1, "one PID lost seven packets in a row");
    assert_eq!(s.cc_lost, 7);
}

#[test]
fn feeds_arrive_over_loopback_paced_and_whole() {
    let clip = Arc::new(Clip::from_bytes("t.ts".into(), synthetic(7000, 2700)).unwrap());
    let base: std::net::SocketAddrV4 = "127.0.0.1:0".parse().unwrap();
    let probe = std::net::UdpSocket::bind(base).unwrap();
    let port = probe.local_addr().unwrap().port();
    drop(probe);
    let socks: Vec<_> = (0..2).map(|i| net::receiver(net::nth(format!("127.0.0.1:{port}").parse().unwrap(), i, false), "127.0.0.1".parse().unwrap(), 1 << 20).unwrap()).collect();
    let feeds = vec![feed(&clip, &format!("127.0.0.1:{port}"), 0.0), feed(&clip, &format!("127.0.0.1:{}", port + 1), 5.0)];
    let totals = Totals::default();
    let start = Instant::now();
    let sender = net::sender("127.0.0.1".parse().unwrap(), 1).unwrap();
    std::thread::scope(|sc| {
        sc.spawn(|| send::run(feeds, sender, start, start + Duration::from_millis(1500), Duration::from_millis(1), &totals));
        let got = super::listen(&socks, Duration::from_millis(1700), Duration::ZERO);
        assert!(got[0].datagrams > 1000, "{} datagrams", got[0].datagrams);
        assert_eq!(got[0].cc_errors, 0);
        assert_eq!(got[0].pcr_jumps, 0);
        assert!(got[1].cc_errors > 0, "five percent loss shows as continuity errors");
    });
    let bytes = totals.bytes.load(std::sync::atomic::Ordering::Relaxed) as f64;
    let rate = clip.kbps() * 1000.0 / 8.0 * 1.5 * 1.95;
    assert!((bytes - rate).abs() / rate < 0.1, "sent {bytes} bytes against about {rate}");
}
