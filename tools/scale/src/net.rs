//! UDP sockets with the options std cannot set: the interface multicast
//! leaves by, port sharing, and a large receive buffer. Unix only, which is
//! where a headend runs this; on Windows std's defaults are used.

use std::net::{Ipv4Addr, SocketAddrV4, UdpSocket};

/// `udp://@239.1.1.1:5000`, `udp://239.1.1.1:5000`, `udp://127.0.0.1:30000`,
/// `udp://@:20001` and `239.1.1.1:5000` all read as an address and port.
pub fn parse(uri: &str) -> Result<SocketAddrV4, String> {
    let s = uri.strip_prefix("udp://").or_else(|| uri.strip_prefix("rtp://")).unwrap_or(uri);
    let s = s.split('?').next().unwrap_or(s);
    let s = s.rsplit('@').next().unwrap_or(s);
    let (host, port) = s.rsplit_once(':').ok_or(format!("{uri} has no port. Write it as udp://239.1.1.1:5000"))?;
    let host: Ipv4Addr = if host.is_empty() { Ipv4Addr::UNSPECIFIED } else { host.parse().map_err(|_| format!("{host} in {uri} is not an IPv4 address"))? };
    let port = port.parse().map_err(|_| format!("{port} in {uri} is not a port"))?;
    Ok(SocketAddrV4::new(host, port))
}

/// The i-th address after `base`: the next group for multicast (and the next
/// port too with `port_step`), the next port for unicast.
pub fn nth(base: SocketAddrV4, i: u32, port_step: bool) -> SocketAddrV4 {
    if base.ip().is_multicast() {
        let port = if port_step { base.port() + i as u16 } else { base.port() };
        SocketAddrV4::new(Ipv4Addr::from(u32::from(*base.ip()) + i), port)
    } else {
        SocketAddrV4::new(*base.ip(), base.port() + i as u16)
    }
}

/// A socket for sending, multicast leaving by `iface` with `ttl`.
pub fn sender(iface: Ipv4Addr, ttl: u32) -> Result<UdpSocket, String> {
    let s = UdpSocket::bind("0.0.0.0:0").map_err(|e| format!("could not open a UDP socket: {e}"))?;
    s.set_multicast_ttl_v4(ttl).map_err(|e| format!("could not set the multicast TTL: {e}"))?;
    s.set_multicast_loop_v4(true).map_err(|e| format!("could not loop multicast back: {e}"))?;
    #[cfg(unix)]
    set(&s, libc::IPPROTO_IP, libc::IP_MULTICAST_IF, libc::in_addr { s_addr: u32::from(iface).to_be() })?;
    #[cfg(unix)]
    set(&s, libc::SOL_SOCKET, libc::SO_SNDBUF, 4i32 << 20)?;
    let _ = iface;
    Ok(s)
}

/// A nonblocking socket receiving `at`: bound to the group itself for
/// multicast (so two groups on one port stay apart) and joined on `iface`.
pub fn receiver(at: SocketAddrV4, iface: Ipv4Addr, buffer: i32) -> Result<UdpSocket, String> {
    let s = shared_bind(at)?;
    if at.ip().is_multicast() {
        s.join_multicast_v4(at.ip(), &iface).map_err(|e| format!("could not join {at} on {iface}: {e}"))?;
    }
    #[cfg(unix)]
    set(&s, libc::SOL_SOCKET, libc::SO_RCVBUF, buffer)?;
    let _ = buffer;
    s.set_nonblocking(true).map_err(|e| format!("could not make {at} nonblocking: {e}"))?;
    Ok(s)
}

#[cfg(unix)]
fn shared_bind(at: SocketAddrV4) -> Result<UdpSocket, String> {
    use std::os::fd::FromRawFd;
    // SAFETY: a plain socket(2); the descriptor is owned by the UdpSocket from here on.
    let fd = unsafe { libc::socket(libc::AF_INET, libc::SOCK_DGRAM, 0) };
    if fd < 0 {
        return Err(format!("could not open a UDP socket: {}", std::io::Error::last_os_error()));
    }
    // SAFETY: fd is a fresh socket nothing else owns.
    let s = unsafe { UdpSocket::from_raw_fd(fd) };
    set(&s, libc::SOL_SOCKET, libc::SO_REUSEADDR, 1i32)?;
    set(&s, libc::SOL_SOCKET, libc::SO_REUSEPORT, 1i32)?;
    // SAFETY: an all zero sockaddr_in is valid, and each field is then set.
    let mut sa: libc::sockaddr_in = unsafe { std::mem::zeroed() };
    sa.sin_family = libc::AF_INET as libc::sa_family_t;
    sa.sin_port = at.port().to_be();
    sa.sin_addr = libc::in_addr { s_addr: u32::from(*at.ip()).to_be() };
    #[cfg(any(target_os = "macos", target_os = "ios", target_os = "freebsd"))]
    {
        sa.sin_len = std::mem::size_of::<libc::sockaddr_in>() as u8;
    }
    let len = std::mem::size_of::<libc::sockaddr_in>() as libc::socklen_t;
    // SAFETY: sa is a valid sockaddr_in of that length.
    if unsafe { libc::bind(fd, &sa as *const _ as *const libc::sockaddr, len) } != 0 {
        return Err(format!("could not listen on {at}: {}. Is another program holding it without sharing?", std::io::Error::last_os_error()));
    }
    Ok(s)
}

#[cfg(not(unix))]
fn shared_bind(at: SocketAddrV4) -> Result<UdpSocket, String> {
    UdpSocket::bind(at).map_err(|e| format!("could not listen on {at}: {e}"))
}

#[cfg(unix)]
fn set<T>(s: &UdpSocket, level: i32, name: i32, value: T) -> Result<(), String> {
    use std::os::fd::AsRawFd;
    let len = std::mem::size_of::<T>() as libc::socklen_t;
    // SAFETY: value lives for the call and len is its size.
    let r = unsafe { libc::setsockopt(s.as_raw_fd(), level, name, &value as *const T as *const libc::c_void, len) };
    if r != 0 {
        return Err(format!("setsockopt {name} failed: {}", std::io::Error::last_os_error()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_every_form_and_counts_on() {
        assert_eq!(parse("udp://@239.1.1.1:5000").unwrap(), "239.1.1.1:5000".parse().unwrap());
        assert_eq!(parse("udp://@:20001?pkt_size=1316").unwrap(), "0.0.0.0:20001".parse().unwrap());
        assert_eq!(parse("127.0.0.1:30000").unwrap(), "127.0.0.1:30000".parse().unwrap());
        assert!(parse("udp://nowhere").is_err());
        assert_eq!(nth("239.1.0.255:5000".parse().unwrap(), 2, false), "239.1.1.1:5000".parse().unwrap());
        assert_eq!(nth("127.0.0.1:30000".parse().unwrap(), 7, false), "127.0.0.1:30007".parse().unwrap());
        assert_eq!(nth("239.1.1.1:5000".parse().unwrap(), 3, true), "239.1.1.4:5003".parse().unwrap());
    }
}
