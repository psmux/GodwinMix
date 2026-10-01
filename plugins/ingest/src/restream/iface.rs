//! The socket options std does not have: which interface multicast leaves
//! by, and a send buffer big enough for a keyframe. The same getifaddrs and
//! setsockopt `plugins/udp/src/iface.rs` uses, for the same reasons.
//!
//! On Windows the interface is chosen by binding to its address instead,
//! which Windows honours for multicast; the send buffer is left as it is.

use std::net::{Ipv4Addr, UdpSocket};

/// An interface's IPv4 address, from its name or the address itself.
pub fn address(iface: &str) -> Result<Ipv4Addr, String> {
    if let Ok(addr) = iface.parse::<Ipv4Addr>() {
        return Ok(addr);
    }
    let all = interfaces();
    all.iter().find(|(n, _)| n == iface).map(|(_, a)| *a).ok_or_else(|| {
        let known: Vec<String> = all.iter().map(|(n, a)| format!("{n} ({a})")).collect();
        format!(
            "there is no interface called '{iface}' with an IPv4 address on this machine. It has: {}. \
             Use one of those, or leave the interface out for the default route.",
            known.join(", ")
        )
    })
}

/// Every interface with an IPv4 address, as `(name, address)`.
#[cfg(unix)]
pub fn interfaces() -> Vec<(String, Ipv4Addr)> {
    let mut out = Vec::new();
    let mut list: *mut libc::ifaddrs = std::ptr::null_mut();
    // SAFETY: getifaddrs fills `list` with a linked list freed below, and
    // every pointer read is checked for null first.
    unsafe {
        if libc::getifaddrs(&mut list) != 0 {
            return out;
        }
        let mut at = list;
        while !at.is_null() {
            let ifa = &*at;
            if !ifa.ifa_addr.is_null() && i32::from((*ifa.ifa_addr).sa_family) == libc::AF_INET {
                let sin = &*(ifa.ifa_addr as *const libc::sockaddr_in);
                let name = std::ffi::CStr::from_ptr(ifa.ifa_name).to_string_lossy().into_owned();
                out.push((name, Ipv4Addr::from(u32::from_be(sin.sin_addr.s_addr))));
            }
            at = ifa.ifa_next;
        }
        libc::freeifaddrs(list);
    }
    out
}

#[cfg(not(unix))]
pub fn interfaces() -> Vec<(String, Ipv4Addr)> {
    Vec::new()
}

#[cfg(unix)]
fn set(socket: &UdpSocket, level: i32, name: i32, value: *const libc::c_void, len: usize) -> std::io::Result<()> {
    use std::os::fd::AsRawFd;
    // SAFETY: the socket is open for as long as `socket` is borrowed, and
    // `value` points at `len` readable bytes the caller owns.
    let r = unsafe { libc::setsockopt(socket.as_raw_fd(), level, name, value, len as libc::socklen_t) };
    if r == 0 { Ok(()) } else { Err(std::io::Error::last_os_error()) }
}

/// Send multicast out of the interface with `addr`.
#[cfg(unix)]
pub fn multicast_from(socket: &UdpSocket, addr: Ipv4Addr) -> Result<(), String> {
    let chosen = libc::in_addr { s_addr: u32::from(addr).to_be() };
    set(socket, libc::IPPROTO_IP, libc::IP_MULTICAST_IF, (&chosen as *const libc::in_addr).cast(), std::mem::size_of::<libc::in_addr>())
        .map_err(|e| format!("could not send multicast out of {addr}: {e}"))
}

#[cfg(not(unix))]
pub fn multicast_from(_socket: &UdpSocket, _addr: Ipv4Addr) -> Result<(), String> {
    Ok(())
}

/// Ask for a send buffer of `bytes`. The kernel may give less; that is fine.
#[cfg(unix)]
pub fn send_buffer(socket: &UdpSocket, bytes: i32) {
    let _ = set(socket, libc::SOL_SOCKET, libc::SO_SNDBUF, (&bytes as *const i32).cast(), std::mem::size_of::<i32>());
}

#[cfg(not(unix))]
pub fn send_buffer(_socket: &UdpSocket, _bytes: i32) {}

/// ENOBUFS: the interface's queue is full, which macOS says rather than
/// blocking a UDP send.
pub fn no_buffer_space(e: &std::io::Error) -> bool {
    #[cfg(unix)]
    {
        e.raw_os_error() == Some(libc::ENOBUFS)
    }
    #[cfg(not(unix))]
    {
        let _ = e;
        false
    }
}
