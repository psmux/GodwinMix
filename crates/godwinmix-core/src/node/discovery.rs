//! mDNS/DNS-SD, so a node on a flat network is found rather than typed.
//!
//! A node advertises `_godwinmix._tcp.local` with a TXT record carrying
//! `role=node`, `api`, and `name`. The core browses for it and offers what it
//! finds to `node.discover`. Neither end depends on it: a network with no
//! multicast (most hosted networks, and plenty of venue wifi) uses the static
//! `[nodes]` table in the core's config instead, and nothing else changes.
//!
//! Written here rather than taken from a crate because it is one service type,
//! one query and one answer, and because every mDNS crate in the registry
//! brings a resolver, a runtime and a cache for a job that is two hundred
//! lines. The DNS wire format is in RFC 1035 and DNS-SD is RFC 6763; what is
//! implemented is the part those two need and no more.

use anyhow::{Context, Result};
use std::collections::BTreeMap;
use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4, UdpSocket};
use std::time::{Duration, Instant};

/// The service type. `_godwinmix._tcp` rather than `_godwinmix-node._tcp`: a
/// core may advertise too, and the TXT record's `role` says which.
pub const SERVICE: &str = "_godwinmix._tcp.local";

const MDNS_GROUP: Ipv4Addr = Ipv4Addr::new(224, 0, 0, 251);
const MDNS_PORT: u16 = 5353;
/// Two minutes. Long enough that a browse does not need to run constantly,
/// short enough that a node that was unplugged stops being offered.
const TTL_SECS: u32 = 120;

const TYPE_A: u16 = 1;
const TYPE_PTR: u16 = 12;
const TYPE_TXT: u16 = 16;
const TYPE_SRV: u16 = 33;
const CLASS_IN: u16 = 1;

/// One thing found on the network.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
pub struct Found {
    /// The instance name, which is the node's name.
    pub name: String,
    /// `host:port`, ready to hand to `godwinmix node --core`.
    pub address: String,
    /// `node` or `core`.
    pub role: String,
    /// The bridge version it speaks.
    pub api: u32,
}

/// Advertise this machine on the local network.
///
/// Holds a socket and answers queries until it is dropped. Nothing else on the
/// node depends on it, so a machine whose network stack refuses multicast logs
/// a warning and carries on.
pub struct Advertisement {
    _worker: std::thread::JoinHandle<()>,
    stop: std::sync::Arc<std::sync::atomic::AtomicBool>,
}

impl Advertisement {
    pub fn start(name: &str, role: &str, port: u16, api: u32) -> Result<Self> {
        let socket = bound_socket().context("open a multicast socket for mDNS")?;
        let record = Record {
            instance: format!("{name}.{SERVICE}"),
            host: format!("{name}.local"),
            port,
            txt: vec![
                format!("role={role}"),
                format!("name={name}"),
                format!("api={api}"),
            ],
            address: local_v4(),
        };
        // Announce twice a second apart, which is what RFC 6762 asks for, then
        // answer whatever is asked.
        let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let mine = stop.clone();
        let worker = std::thread::Builder::new()
            .name("gmx-mdns".into())
            .spawn(move || respond(socket, record, mine))
            .context("start the mDNS thread")?;
        Ok(Self { _worker: worker, stop })
    }
}

impl Drop for Advertisement {
    fn drop(&mut self) {
        self.stop.store(true, std::sync::atomic::Ordering::Relaxed);
    }
}

/// Ask the network who is there, and collect answers for `within`.
pub fn browse(within: Duration) -> Result<Vec<Found>> {
    let socket = bound_socket().context("open a multicast socket for mDNS")?;
    socket.set_read_timeout(Some(Duration::from_millis(200)))?;
    let query = question(SERVICE, TYPE_PTR);
    socket
        .send_to(&query, SocketAddrV4::new(MDNS_GROUP, MDNS_PORT))
        .context("send the mDNS question")?;
    let deadline = Instant::now() + within;
    let mut instances: BTreeMap<String, Partial> = BTreeMap::new();
    let mut buf = [0u8; 4096];
    while Instant::now() < deadline {
        let Ok((len, _)) = socket.recv_from(&mut buf) else { continue };
        absorb(&buf[..len], &mut instances);
    }
    Ok(instances.into_values().filter_map(Partial::finish).collect())
}

#[derive(Default)]
struct Partial {
    name: String,
    host: String,
    port: u16,
    role: String,
    api: u32,
    address: Option<Ipv4Addr>,
}

impl Partial {
    fn finish(self) -> Option<Found> {
        if self.name.is_empty() || self.port == 0 {
            return None;
        }
        let host = match self.address {
            Some(ip) => ip.to_string(),
            None if !self.host.is_empty() => self.host.trim_end_matches('.').to_string(),
            None => return None,
        };
        Some(Found {
            name: self.name,
            address: format!("{host}:{}", self.port),
            role: if self.role.is_empty() { "node".into() } else { self.role },
            api: self.api,
        })
    }
}

/// Answer PTR questions for our service until told to stop.
fn respond(socket: UdpSocket, record: Record, stop: std::sync::Arc<std::sync::atomic::AtomicBool>) {
    let _ = socket.set_read_timeout(Some(Duration::from_millis(500)));
    let announcement = record.answer();
    for _ in 0..2 {
        let _ = socket.send_to(&announcement, SocketAddrV4::new(MDNS_GROUP, MDNS_PORT));
        std::thread::sleep(Duration::from_secs(1));
        if stop.load(std::sync::atomic::Ordering::Relaxed) {
            return;
        }
    }
    let mut buf = [0u8; 4096];
    while !stop.load(std::sync::atomic::Ordering::Relaxed) {
        let Ok((len, from)) = socket.recv_from(&mut buf) else { continue };
        if !asks_for_us(&buf[..len]) {
            continue;
        }
        let to: SocketAddr = match from {
            SocketAddr::V4(_) => SocketAddrV4::new(MDNS_GROUP, MDNS_PORT).into(),
            other => other,
        };
        let _ = socket.send_to(&announcement, to);
    }
}

struct Record {
    instance: String,
    host: String,
    port: u16,
    txt: Vec<String>,
    address: Option<Ipv4Addr>,
}

impl Record {
    /// One message carrying PTR, SRV, TXT and, when we know it, A.
    ///
    /// No name compression. It costs a few dozen bytes on a packet sent twice
    /// a minute and it makes this readable.
    fn answer(&self) -> Vec<u8> {
        let mut answers = Vec::new();
        answers.push(resource(SERVICE, TYPE_PTR, &name_bytes(&self.instance)));
        let mut srv = Vec::new();
        srv.extend_from_slice(&0u16.to_be_bytes()); // priority
        srv.extend_from_slice(&0u16.to_be_bytes()); // weight
        srv.extend_from_slice(&self.port.to_be_bytes());
        srv.extend_from_slice(&name_bytes(&self.host));
        answers.push(resource(&self.instance, TYPE_SRV, &srv));
        let mut txt = Vec::new();
        for entry in &self.txt {
            txt.push(entry.len().min(255) as u8);
            txt.extend_from_slice(&entry.as_bytes()[..entry.len().min(255)]);
        }
        answers.push(resource(&self.instance, TYPE_TXT, &txt));
        if let Some(ip) = self.address {
            answers.push(resource(&self.host, TYPE_A, &ip.octets()));
        }
        let mut out = Vec::new();
        out.extend_from_slice(&0u16.to_be_bytes()); // id
        out.extend_from_slice(&0x8400u16.to_be_bytes()); // response, authoritative
        out.extend_from_slice(&0u16.to_be_bytes()); // questions
        out.extend_from_slice(&(answers.len() as u16).to_be_bytes());
        out.extend_from_slice(&0u16.to_be_bytes()); // authority
        out.extend_from_slice(&0u16.to_be_bytes()); // additional
        for a in answers {
            out.extend_from_slice(&a);
        }
        out
    }
}

fn resource(name: &str, kind: u16, data: &[u8]) -> Vec<u8> {
    let mut out = name_bytes(name);
    out.extend_from_slice(&kind.to_be_bytes());
    out.extend_from_slice(&CLASS_IN.to_be_bytes());
    out.extend_from_slice(&TTL_SECS.to_be_bytes());
    out.extend_from_slice(&(data.len() as u16).to_be_bytes());
    out.extend_from_slice(data);
    out
}

fn question(name: &str, kind: u16) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(&0u16.to_be_bytes());
    out.extend_from_slice(&0u16.to_be_bytes()); // a query
    out.extend_from_slice(&1u16.to_be_bytes()); // one question
    out.extend_from_slice(&[0, 0, 0, 0, 0, 0]);
    out.extend_from_slice(&name_bytes(name));
    out.extend_from_slice(&kind.to_be_bytes());
    out.extend_from_slice(&CLASS_IN.to_be_bytes());
    out
}

/// A DNS name as length prefixed labels, terminated by a zero byte.
fn name_bytes(name: &str) -> Vec<u8> {
    let mut out = Vec::with_capacity(name.len() + 2);
    for label in name.trim_end_matches('.').split('.') {
        let label = &label.as_bytes()[..label.len().min(63)];
        out.push(label.len() as u8);
        out.extend_from_slice(label);
    }
    out.push(0);
    out
}

/// Whether a message asks a question about our service type.
fn asks_for_us(message: &[u8]) -> bool {
    let Some(reader) = Reader::open(message) else { return false };
    let asked = reader.questions().collect::<Vec<_>>();
    asked.iter().any(|(name, _)| name.eq_ignore_ascii_case(SERVICE))
}

/// Fold every record in one message into what we know about each instance.
fn absorb(message: &[u8], into: &mut BTreeMap<String, Partial>) {
    let Some(reader) = Reader::open(message) else { return };
    let mut addresses: BTreeMap<String, Ipv4Addr> = BTreeMap::new();
    for (name, kind, data) in reader.records() {
        match kind {
            TYPE_PTR => {
                if !name.eq_ignore_ascii_case(SERVICE) {
                    continue;
                }
                if let Some(instance) = read_name(&data, 0).map(|(n, _)| n) {
                    let short = instance
                        .strip_suffix(&format!(".{SERVICE}"))
                        .unwrap_or(&instance)
                        .to_string();
                    into.entry(instance.clone()).or_default().name = short;
                }
            }
            TYPE_SRV => {
                if data.len() < 7 {
                    continue;
                }
                let entry = into.entry(name.clone()).or_default();
                entry.port = u16::from_be_bytes([data[4], data[5]]);
                if let Some((host, _)) = read_name(&data, 6) {
                    entry.host = host;
                }
                if entry.name.is_empty() {
                    entry.name =
                        name.strip_suffix(&format!(".{SERVICE}")).unwrap_or(&name).to_string();
                }
            }
            TYPE_TXT => {
                let entry = into.entry(name.clone()).or_default();
                for (key, value) in text_pairs(&data) {
                    match key.as_str() {
                        "role" => entry.role = value,
                        "api" => entry.api = value.parse().unwrap_or(0),
                        "name" if entry.name.is_empty() => entry.name = value,
                        _ => {}
                    }
                }
            }
            TYPE_A if data.len() == 4 => {
                addresses.insert(
                    name.trim_end_matches('.').to_string(),
                    Ipv4Addr::new(data[0], data[1], data[2], data[3]),
                );
            }
            _ => {}
        }
    }
    for entry in into.values_mut() {
        if entry.address.is_none() {
            entry.address = addresses.get(entry.host.trim_end_matches('.')).copied();
        }
    }
}

fn text_pairs(data: &[u8]) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut at = 0;
    while at < data.len() {
        let len = data[at] as usize;
        at += 1;
        if at + len > data.len() {
            break;
        }
        if let Ok(text) = std::str::from_utf8(&data[at..at + len]) {
            if let Some((k, v)) = text.split_once('=') {
                out.push((k.to_string(), v.to_string()));
            }
        }
        at += len;
    }
    out
}

/// Just enough of a DNS message reader for one service type.
struct Reader<'a> {
    message: &'a [u8],
    questions: u16,
    answers: u16,
}

impl<'a> Reader<'a> {
    fn open(message: &'a [u8]) -> Option<Self> {
        if message.len() < 12 {
            return None;
        }
        let questions = u16::from_be_bytes([message[4], message[5]]);
        let answers = u16::from_be_bytes([message[6], message[7]])
            + u16::from_be_bytes([message[8], message[9]])
            + u16::from_be_bytes([message[10], message[11]]);
        Some(Self { message, questions, answers })
    }

    fn questions(&self) -> impl Iterator<Item = (String, u16)> + '_ {
        let mut at = 12;
        let count = self.questions;
        (0..count).filter_map(move |_| {
            let (name, next) = read_name(self.message, at)?;
            if next + 4 > self.message.len() {
                return None;
            }
            let kind = u16::from_be_bytes([self.message[next], self.message[next + 1]]);
            at = next + 4;
            Some((name, kind))
        })
    }

    fn records(&self) -> Vec<(String, u16, Vec<u8>)> {
        let mut at = 12;
        for _ in 0..self.questions {
            let Some((_, next)) = read_name(self.message, at) else { return Vec::new() };
            at = next + 4;
        }
        let mut out = Vec::new();
        for _ in 0..self.answers {
            let Some((name, next)) = read_name(self.message, at) else { break };
            if next + 10 > self.message.len() {
                break;
            }
            let kind = u16::from_be_bytes([self.message[next], self.message[next + 1]]);
            let len =
                u16::from_be_bytes([self.message[next + 8], self.message[next + 9]]) as usize;
            let start = next + 10;
            if start + len > self.message.len() {
                break;
            }
            out.push((name, kind, self.message[start..start + len].to_vec()));
            at = start + len;
        }
        out
    }
}

/// Read a name, following one level of compression pointer.
///
/// Returns the name and where the caller should carry on, which for a
/// compressed name is two bytes past the pointer rather than wherever it led.
fn read_name(message: &[u8], mut at: usize) -> Option<(String, usize)> {
    let mut labels = Vec::new();
    let mut after = None;
    let mut hops = 0;
    loop {
        if at >= message.len() || hops > 8 {
            return None;
        }
        let len = message[at] as usize;
        if len == 0 {
            at += 1;
            break;
        }
        if len & 0xC0 == 0xC0 {
            if at + 1 >= message.len() {
                return None;
            }
            let target = (((len & 0x3F) << 8) | message[at + 1] as usize) & 0x3FFF;
            after.get_or_insert(at + 2);
            at = target;
            hops += 1;
            continue;
        }
        if at + 1 + len > message.len() {
            return None;
        }
        labels.push(String::from_utf8_lossy(&message[at + 1..at + 1 + len]).into_owned());
        at += 1 + len;
    }
    Some((labels.join("."), after.unwrap_or(at)))
}

/// A socket bound to the mDNS port and joined to the group.
///
/// `SO_REUSEADDR` because the operating system's own responder is almost
/// always already there, and a node that refused to start because Bonjour
/// exists would be a node nobody could run on a Mac.
fn bound_socket() -> Result<UdpSocket> {
    let socket = reuse_socket()?;
    socket
        .join_multicast_v4(&MDNS_GROUP, &Ipv4Addr::UNSPECIFIED)
        .context("join the mDNS multicast group")?;
    socket.set_multicast_loop_v4(true)?;
    Ok(socket)
}

#[cfg(unix)]
fn reuse_socket() -> Result<UdpSocket> {
    use std::os::fd::FromRawFd;
    // SAFETY: the descriptor comes from `socket(2)` on this line and is handed
    // straight to `UdpSocket`, which owns and closes it.
    unsafe {
        let fd = libc::socket(libc::AF_INET, libc::SOCK_DGRAM, 0);
        anyhow::ensure!(fd >= 0, "could not open a UDP socket for mDNS");
        let on: libc::c_int = 1;
        let value = std::ptr::addr_of!(on).cast::<libc::c_void>();
        let size = std::mem::size_of::<libc::c_int>() as libc::socklen_t;
        libc::setsockopt(fd, libc::SOL_SOCKET, libc::SO_REUSEADDR, value, size);
        #[cfg(any(target_os = "macos", target_os = "ios", target_os = "freebsd"))]
        libc::setsockopt(fd, libc::SOL_SOCKET, libc::SO_REUSEPORT, value, size);
        let addr = libc::sockaddr_in {
            sin_family: libc::AF_INET as libc::sa_family_t,
            sin_port: MDNS_PORT.to_be(),
            sin_addr: libc::in_addr { s_addr: 0 },
            ..std::mem::zeroed()
        };
        let bound = libc::bind(
            fd,
            (&addr as *const libc::sockaddr_in).cast(),
            std::mem::size_of::<libc::sockaddr_in>() as libc::socklen_t,
        );
        if bound != 0 {
            libc::close(fd);
            anyhow::bail!(
                "could not bind the mDNS port. Something else has it exclusively; use the \
                 [nodes] table in the config instead of discovery"
            );
        }
        Ok(UdpSocket::from_raw_fd(fd))
    }
}

#[cfg(not(unix))]
fn reuse_socket() -> Result<UdpSocket> {
    // Windows allows two binders on a UDP port without any option being set on
    // the second one, so the plain constructor is enough here.
    UdpSocket::bind((Ipv4Addr::UNSPECIFIED, MDNS_PORT))
        .context("bind the mDNS port")
}

/// This machine's first non loopback IPv4 address, or nothing.
///
/// Worked out by asking the routing table where a packet to a public address
/// would leave from, which needs no interface enumeration and no crate. The
/// socket is never connected to anything and sends nothing.
fn local_v4() -> Option<Ipv4Addr> {
    let socket = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0)).ok()?;
    socket.connect((Ipv4Addr::new(192, 0, 2, 1), 9)).ok()?;
    match socket.local_addr().ok()? {
        SocketAddr::V4(v4) if !v4.ip().is_loopback() => Some(*v4.ip()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_name_round_trips_through_the_wire_format() {
        let bytes = name_bytes("studio-b._godwinmix._tcp.local");
        let (back, next) = read_name(&bytes, 0).unwrap();
        assert_eq!(back, "studio-b._godwinmix._tcp.local");
        assert_eq!(next, bytes.len());
    }

    #[test]
    fn an_announcement_reads_back_as_what_was_put_in() {
        let record = Record {
            instance: format!("studio-b.{SERVICE}"),
            host: "studio-b.local".into(),
            port: 8443,
            txt: vec!["role=node".into(), "name=studio-b".into(), "api=1".into()],
            address: Some(Ipv4Addr::new(10, 0, 0, 21)),
        };
        let message = record.answer();
        let mut found = BTreeMap::new();
        absorb(&message, &mut found);
        let all: Vec<Found> = found.into_values().filter_map(Partial::finish).collect();
        assert_eq!(all.len(), 1, "one instance, got {all:?}");
        assert_eq!(all[0].name, "studio-b");
        assert_eq!(all[0].address, "10.0.0.21:8443");
        assert_eq!(all[0].role, "node");
        assert_eq!(all[0].api, 1);
    }

    #[test]
    fn a_question_about_our_service_is_recognised() {
        assert!(asks_for_us(&question(SERVICE, TYPE_PTR)));
        assert!(!asks_for_us(&question("_http._tcp.local", TYPE_PTR)));
    }

    #[test]
    fn a_truncated_message_is_ignored_rather_than_panicking() {
        let record = Record {
            instance: format!("a.{SERVICE}"),
            host: "a.local".into(),
            port: 1,
            txt: vec!["role=node".into()],
            address: None,
        };
        let full = record.answer();
        for cut in 0..full.len() {
            let mut found = BTreeMap::new();
            absorb(&full[..cut], &mut found);
            assert!(!asks_for_us(&full[..cut]) || cut >= 12);
        }
    }
}
