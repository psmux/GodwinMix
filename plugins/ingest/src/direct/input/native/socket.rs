//! The one socket of a group: bound, joined on the interface asked for
//! (source specific where a source is named), with a receive buffer that
//! holds a second or so of a full multiplex.
//!
//! The interface is named as the udp plugin names it (`en1`, `eth1`, or one
//! of its IPv4 addresses, `gmx_udp::iface`). A multicast socket is bound to
//! the group itself on Linux and macOS, so a second group on the same port
//! is not delivered to it, and shares its port with any other receiver of
//! the group on the machine. A unicast port is not shared: a second
//! receiver on it is a mistake, and the error says so.

use std::io;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::time::Duration;

use gmx_udp::address::Endpoint;
use socket2::{Domain, Protocol, Socket, Type};

/// How long a receive waits before the thread looks round again.
pub const WAIT: Duration = Duration::from_millis(250);

/// Open `e` on `iface` (empty for the default route) with a receive buffer
/// of `buffer` bytes. Answers the socket and the buffer the OS gave.
pub fn open(e: &Endpoint, iface: &str, buffer: usize) -> Result<(Socket, usize), String> {
    let host: IpAddr = e.host.parse().map_err(|_| format!("'{}' is not an IP address. Write the group or the local address as numbers, udp://@239.1.1.1:5000.", e.host))?;
    let fail = |what: &str, err: io::Error| format!("could not {what} for {}: {err}", e.display("udp"));
    let s = Socket::new(Domain::for_address(SocketAddr::new(host, e.port)), Type::DGRAM, Some(Protocol::UDP)).map_err(|x| fail("open a socket", x))?;
    let multicast = host.is_multicast();
    if multicast {
        s.set_reuse_address(true).map_err(|x| fail("share the port", x))?;
        #[cfg(unix)]
        s.set_reuse_port(true).map_err(|x| fail("share the port", x))?;
    }
    let got = buffer_of(&s, buffer);
    let bind = if multicast && !cfg!(unix) { unspecified(host) } else { host };
    s.bind(&SocketAddr::new(bind, e.port).into()).map_err(|x| fail(&format!("bind port {}", e.port), x))?;
    if multicast {
        join(&s, host, e, iface)?;
    }
    s.set_read_timeout(Some(WAIT)).map_err(|x| fail("set a timeout", x))?;
    Ok((s, got))
}

fn unspecified(host: IpAddr) -> IpAddr {
    if host.is_ipv4() { IpAddr::V4(Ipv4Addr::UNSPECIFIED) } else { IpAddr::V6(std::net::Ipv6Addr::UNSPECIFIED) }
}

/// Ask for `want` bytes, past the OS ceiling where the process may.
fn buffer_of(s: &Socket, want: usize) -> usize {
    let _ = s.set_recv_buffer_size(want);
    #[cfg(target_os = "linux")]
    if s.recv_buffer_size().unwrap_or(0) < want {
        force_buffer(s, want);
    }
    s.recv_buffer_size().unwrap_or(0)
}

/// SO_RCVBUFFORCE: past `net.core.rmem_max`, with CAP_NET_ADMIN. Without
/// it the call fails and the buffer stays what the ceiling allowed.
#[cfg(target_os = "linux")]
fn force_buffer(s: &Socket, want: usize) {
    use std::os::fd::AsRawFd;
    let size = want.min(i32::MAX as usize) as libc::c_int;
    // SAFETY: a live socket's descriptor, and an int option of its true size.
    unsafe {
        libc::setsockopt(s.as_raw_fd(), libc::SOL_SOCKET, libc::SO_RCVBUFFORCE, (&size as *const libc::c_int).cast(), std::mem::size_of::<libc::c_int>() as libc::socklen_t);
    }
}

fn join(s: &Socket, group: IpAddr, e: &Endpoint, iface: &str) -> Result<(), String> {
    let on = if iface.is_empty() { String::new() } else { format!(" on {iface}") };
    let fail = |err: io::Error| format!("could not join {}{on}: {err}", e.display("udp"));
    match group {
        IpAddr::V4(g) => {
            let local = if iface.is_empty() { Ipv4Addr::UNSPECIFIED } else { gmx_udp::iface::address_of(iface).ok_or_else(|| no_such(iface))? };
            match &e.source {
                Some(src) => {
                    let src: Ipv4Addr = src.parse().map_err(|_| format!("the source '{src}' is not an IPv4 address"))?;
                    s.join_ssm_v4(&src, &g, &local).map_err(fail)
                }
                None => s.join_multicast_v4(&g, &local).map_err(fail),
            }
        }
        IpAddr::V6(g) => s.join_multicast_v6(&g, index_of(iface)).map_err(fail),
    }
}

fn no_such(iface: &str) -> String {
    let known: Vec<String> = gmx_udp::iface::interfaces().into_iter().map(|(n, a)| format!("{n} ({a})")).collect();
    format!("there is no interface called '{iface}' with an IPv4 address on this machine. It has: {}. Use one of those names, or leave the interface empty for the default route.", known.join(", "))
}

#[cfg(unix)]
fn index_of(iface: &str) -> u32 {
    let Ok(name) = std::ffi::CString::new(gmx_udp::iface::name_of(iface)) else { return 0 };
    // SAFETY: a NUL terminated name; 0 back means no such interface.
    unsafe { libc::if_nametoindex(name.as_ptr()) }
}

#[cfg(not(unix))]
fn index_of(_iface: &str) -> u32 {
    0
}
