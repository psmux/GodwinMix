//! Choosing the network interface multicast uses, which is the one part of
//! this plugin that differs by platform.
//!
//! Receiving is GLib's: `udpsrc` joins the group on the interface named in
//! `multicast-iface`, and that works on macOS and Linux as it is. Sending is
//! not. `udpsink`'s `multicast-iface` is used only to join, never to choose
//! where packets leave, so a machine with a separate media network sent its
//! multicast out of the default route whatever the setting said (measured on
//! macOS with GStreamer 1.28.7). The fix is the socket option itself,
//! `IP_MULTICAST_IF`, set on the socket `udpsink` made, with the interface's
//! IPv4 address. That needs `getifaddrs` and `setsockopt`, so it is Unix only;
//! on Windows the setting is passed to `udpsink` as before and the reference
//! page says it has not been tried there.
//!
//! An interface may be written as a name (`en1`, `eth1`) or as one of its IPv4
//! addresses (`10.0.0.5`). Both ends accept both.

use std::net::Ipv4Addr;

use gstreamer as gst;
#[cfg(unix)]
use gstreamer::glib;
#[cfg(unix)]
use gstreamer::prelude::*;

/// The IPv4 address of `iface`: itself if it is one, else the first IPv4
/// address the named interface has.
pub fn address_of(iface: &str) -> Option<Ipv4Addr> {
    iface.parse().ok().or_else(|| interfaces().into_iter().find(|(n, _)| n == iface).map(|(_, a)| a))
}

/// The name of `iface`: itself unless it is an address, in which case the
/// interface that has it. `udpsrc` wants a name.
pub fn name_of(iface: &str) -> String {
    let Ok(addr) = iface.parse::<Ipv4Addr>() else { return iface.to_string() };
    interfaces().into_iter().find(|(_, a)| *a == addr).map(|(n, _)| n).unwrap_or_else(|| iface.to_string())
}

/// Every interface with an IPv4 address, as `(name, address)`.
#[cfg(unix)]
pub fn interfaces() -> Vec<(String, Ipv4Addr)> {
    let mut out = Vec::new();
    let mut list: *mut libc::ifaddrs = std::ptr::null_mut();
    // SAFETY: getifaddrs fills `list` with a linked list we free below, and
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

/// Make the socket `udpsink` opened send multicast out of `iface`. Call it
/// after the sink has started and before anything is sent.
#[cfg(unix)]
pub fn send_multicast_out(sink: &gst::Element, iface: &str) -> Result<(), String> {
    let addr = address_of(iface).ok_or_else(|| {
        let known: Vec<String> = interfaces().into_iter().map(|(n, a)| format!("{n} ({a})")).collect();
        format!(
            "there is no interface called '{iface}' with an IPv4 address on this machine. It has: {}. \
             Use one of those names, or leave the interface empty for the default route.",
            known.join(", ")
        )
    })?;
    let socket = sink
        .property::<Option<glib::Object>>("used-socket")
        .ok_or("udpsink has not opened its socket yet, so the interface cannot be set")?;
    let fd: i32 = socket.property("fd");
    let chosen = libc::in_addr { s_addr: u32::from(addr).to_be() };
    // SAFETY: `fd` is the live socket udpsink owns, and the option value is
    // an in_addr on the stack with its true size.
    let r = unsafe {
        libc::setsockopt(
            fd,
            libc::IPPROTO_IP,
            libc::IP_MULTICAST_IF,
            (&chosen as *const libc::in_addr).cast(),
            std::mem::size_of::<libc::in_addr>() as libc::socklen_t,
        )
    };
    if r != 0 {
        return Err(format!(
            "could not send multicast out of {iface} ({addr}): {}",
            std::io::Error::last_os_error()
        ));
    }
    Ok(())
}

#[cfg(not(unix))]
pub fn send_multicast_out(_sink: &gst::Element, _iface: &str) -> Result<(), String> {
    Ok(())
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    #[test]
    fn the_loopback_is_found_by_name_and_by_address() {
        let lo = interfaces().into_iter().find(|(_, a)| a.is_loopback()).expect("a loopback interface");
        assert_eq!(address_of(&lo.0), Some(lo.1));
        assert_eq!(name_of(&lo.1.to_string()), lo.0);
        assert_eq!(name_of("eth7"), "eth7", "a name is left alone");
    }
}
