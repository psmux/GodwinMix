//! MPEG-TS over UDP, unicast or multicast: the tags remuxed by
//! `crate::tsmux` and sent seven packets to a datagram, from the sender's
//! own thread. No GStreamer, no second thread.
//!
//! `udp://239.1.1.1:5000?ttl=4&interface=en0` for multicast (`interface`
//! is a name or an address; `ttl` defaults to 16), `udp://10.0.0.9:5000`
//! for one receiver. A send the kernel has no room for is a lost datagram,
//! counted, never a wait: UDP has nobody to wait for.

use std::net::{SocketAddr, ToSocketAddrs, UdpSocket};

use crate::media_tag::MediaTag;
use crate::tsmux::{Muxer, PACKET};

use super::link::{Failure, Link};
use super::target::Target;

/// Seven packets, the size every receiver and every switch expects.
const DATAGRAM: usize = 7 * PACKET;

pub struct UdpLink {
    socket: UdpSocket,
    to: SocketAddr,
    muxer: Muxer,
    buf: Vec<u8>,
    name: String,
    /// Datagrams the kernel would not take.
    pub lost: u64,
}

/// An address's query, as `key=value` pairs.
pub fn query(url: &str) -> Vec<(String, String)> {
    let Some((_, q)) = url.split_once('?') else { return Vec::new() };
    q.split('&').filter_map(|kv| kv.split_once('=')).map(|(k, v)| (k.to_ascii_lowercase(), v.to_string())).collect()
}

/// `host:port` out of `scheme://host:port/...?...`, with `@` (a receiver's
/// way of writing "listen") taken off.
pub fn host_port(url: &str) -> Result<String, String> {
    let rest = url.split_once("://").map(|(_, r)| r).unwrap_or(url);
    let rest = rest.split(['?', '/']).next().unwrap_or("").trim_start_matches('@');
    if !rest.contains(':') {
        return Err(format!("'{url}' needs a port, as in udp://239.1.1.1:5000"));
    }
    Ok(rest.to_string())
}

impl UdpLink {
    pub fn dial(target: &Target) -> Result<UdpLink, Failure> {
        let hp = host_port(&target.url).map_err(Failure::Refused)?;
        let to = hp
            .to_socket_addrs()
            .map_err(|e| Failure::Lost(format!("{hp} does not resolve: {e}")))?
            .next()
            .ok_or_else(|| Failure::Lost(format!("{hp} does not resolve")))?;
        let options = query(&target.url);
        let get = |k: &str| options.iter().find(|(key, _)| key == k).map(|(_, v)| v.clone());
        let iface = match get("interface").or_else(|| get("iface")) {
            Some(i) => Some(super::iface::address(&i).map_err(Failure::Refused)?),
            None => None,
        };
        // Bound to the interface's address when one is named: the source
        // address a receiver sees, and on Windows the way multicast is routed.
        let local: SocketAddr = match (to, iface) {
            (SocketAddr::V4(_), Some(ip)) => (ip, 0).into(),
            (SocketAddr::V4(_), None) => ([0, 0, 0, 0], 0).into(),
            (SocketAddr::V6(_), _) => (std::net::Ipv6Addr::UNSPECIFIED, 0).into(),
        };
        let socket = UdpSocket::bind(local).map_err(|e| Failure::Lost(format!("no UDP socket on {local}: {e}")))?;
        if let SocketAddr::V4(v4) = to {
            if v4.ip().is_multicast() {
                let ttl = get("ttl").and_then(|t| t.parse().ok()).unwrap_or(16);
                socket.set_multicast_ttl_v4(ttl).map_err(|e| Failure::Refused(format!("ttl {ttl}: {e}")))?;
                if let Some(ip) = iface {
                    super::iface::multicast_from(&socket, ip).map_err(Failure::Refused)?;
                }
            }
        }
        super::iface::send_buffer(&socket, 4 * 1024 * 1024);
        Ok(UdpLink { socket, to, muxer: Muxer::new(), buf: Vec::with_capacity(64 * 1024), name: target.name(), lost: 0 })
    }

    fn flush(&mut self) -> Result<usize, Failure> {
        let mut sent = 0;
        for chunk in self.buf.chunks(DATAGRAM) {
            match self.socket.send_to(chunk, self.to) {
                Ok(n) => sent += n,
                Err(e) if lost_not_failed(&e) => self.lost += 1,
                Err(e) => return Err(Failure::Lost(format!("{} stopped taking datagrams: {e}", self.name))),
            }
        }
        self.buf.clear();
        Ok(sent)
    }
}

/// The kernel's queue was full, or nobody is listening on a unicast port:
/// for UDP that is a datagram lost, not a link gone.
fn lost_not_failed(e: &std::io::Error) -> bool {
    use std::io::ErrorKind::*;
    matches!(e.kind(), WouldBlock | ConnectionRefused) || super::iface::no_buffer_space(e)
}

impl Link for UdpLink {
    fn send(&mut self, tag: &MediaTag, timestamp_ms: u32) -> Result<usize, Failure> {
        self.muxer.tag(tag, timestamp_ms, &mut self.buf);
        if self.buf.is_empty() {
            return Ok(0);
        }
        self.flush()
    }

    fn poll(&mut self) -> Result<(), Failure> {
        Ok(())
    }

    fn close(&mut self) {}
}
