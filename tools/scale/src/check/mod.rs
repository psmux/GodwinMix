//! `gmx-scale check`: receive N UDP streams and say what arrived.

pub mod report;
pub mod stream;
#[cfg(test)]
mod tests;

use crate::args::Args;
use crate::net;
use std::net::UdpSocket;
use std::time::{Duration, Instant};
use stream::Stream;

const HELP: &str = "
gmx-scale check --from ADDR --count N [options]

Listens on --count addresses, the i-th group after --from (or the i-th port for
a unicast address), and counts what arrives on each: datagrams, TS packets,
continuity errors, PCR jumps and spacing, PCR jitter, keyframes and GOPs lost.

  --from ADDR        udp://127.0.0.1:30000 by default
  --count N          how many streams (200)
  --seconds S        how long to listen (60)
  --port-step        multicast: the next port as well as the next group per stream
  --iface IP         the interface to join multicast on (127.0.0.1)
  --threads N        receiving threads, each with its share of the sockets (4)
  --skip S           seconds at the start not counted, while outputs settle (0)
  --json FILE        write every stream and the total there as JSON
  --quiet            print the total only, not a line per stream
";

pub fn main(a: Args) -> Result<(), String> {
    if a.help(HELP) {
        return Ok(());
    }
    let from = net::parse(a.str("from").unwrap_or("udp://127.0.0.1:30000"))?;
    let count = a.num("count", 200u32)?;
    let iface = a.str("iface").unwrap_or("127.0.0.1").parse().map_err(|_| "--iface takes an IPv4 address".to_string())?;
    let socks = (0..count).map(|i| net::receiver(net::nth(from, i, a.flag("port-step")), iface, 4 << 20)).collect::<Result<Vec<_>, _>>()?;
    let seconds = a.num("seconds", 60.0f64)?;
    let skip = Duration::from_secs_f64(a.num("skip", 0.0f64)?);
    let threads = a.num("threads", 4usize)?.clamp(1, socks.len().max(1));
    let streams = listen_on(&socks, threads, Duration::from_secs_f64(seconds), skip);
    let names: Vec<String> = (0..count).map(|i| net::nth(from, i, a.flag("port-step")).to_string()).collect();
    let mut out = report::json(&names, &streams, seconds - skip.as_secs_f64());
    out["total"]["checker_cpu_percent"] = serde_json::json!((crate::procs::own_cpu_seconds() / seconds * 1000.0).round() / 10.0);
    report::print(&out, a.flag("quiet"));
    if let Some(path) = a.str("json") {
        std::fs::write(path, format!("{out}\n")).map_err(|e| format!("could not write {path}: {e}"))?;
    }
    Ok(())
}

/// Splits the sockets over `threads` threads, each listening to its share.
fn listen_on(socks: &[UdpSocket], threads: usize, length: Duration, skip: Duration) -> Vec<Stream> {
    let per = socks.len().div_ceil(threads).max(1);
    std::thread::scope(|s| {
        let parts: Vec<_> = socks.chunks(per).map(|c| s.spawn(move || listen(c, length, skip))).collect();
        parts.into_iter().flat_map(|h| h.join().unwrap_or_default()).collect()
    })
}

/// Polls every socket until `length` is up, reading each dry when it wakes.
fn listen(socks: &[UdpSocket], length: Duration, skip: Duration) -> Vec<Stream> {
    let mut streams: Vec<Stream> = socks.iter().map(|_| Stream::new()).collect();
    let mut buf = vec![0u8; 65536];
    let start = Instant::now();
    let counted = start + skip;
    let fds = poll_set(socks);
    let mut fds = fds;
    while start.elapsed() < length {
        if !wait(&mut fds) {
            continue;
        }
        let now = Instant::now();
        let counting = now >= counted;
        let ms = now.saturating_duration_since(counted).as_secs_f64() * 1000.0;
        for (i, s) in socks.iter().enumerate() {
            if !ready(&fds, i) {
                continue;
            }
            while let Ok(n) = s.recv(&mut buf) {
                if counting {
                    streams[i].datagram(&buf[..n], ms);
                }
            }
        }
    }
    streams
}

#[cfg(unix)]
type PollSet = Vec<libc::pollfd>;
#[cfg(not(unix))]
type PollSet = ();

#[cfg(unix)]
fn poll_set(socks: &[UdpSocket]) -> PollSet {
    use std::os::fd::AsRawFd;
    socks.iter().map(|s| libc::pollfd { fd: s.as_raw_fd(), events: libc::POLLIN, revents: 0 }).collect()
}

/// Waits up to 20 ms for any socket to have something.
#[cfg(unix)]
fn wait(fds: &mut PollSet) -> bool {
    // SAFETY: fds is a live slice of pollfd of the length given.
    unsafe { libc::poll(fds.as_mut_ptr(), fds.len() as libc::nfds_t, 20) > 0 }
}

#[cfg(unix)]
fn ready(fds: &PollSet, i: usize) -> bool {
    fds[i].revents & libc::POLLIN != 0
}

#[cfg(not(unix))]
fn poll_set(_: &[UdpSocket]) -> PollSet {}

#[cfg(not(unix))]
fn wait(_: &mut PollSet) -> bool {
    std::thread::sleep(Duration::from_millis(2));
    true
}

#[cfg(not(unix))]
fn ready(_: &PollSet, _: usize) -> bool {
    true
}
