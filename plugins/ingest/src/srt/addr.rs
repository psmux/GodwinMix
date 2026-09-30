//! `struct sockaddr` by hand, so the SRT listener needs no libc crate.
//!
//! The one difference that matters between platforms: the BSDs (macOS
//! among them) start the struct with a length byte and a one byte family,
//! and Linux and Windows with a two byte family. The family numbers for IPv4
//! agree everywhere; IPv6 does not, so it is read by all three numbers.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};

const BSD: bool = cfg!(any(target_os = "macos", target_os = "ios", target_os = "freebsd", target_os = "openbsd"));
const AF_INET: u8 = 2;
const AF_INET6: u8 = if cfg!(any(target_os = "macos", target_os = "ios")) {
    30
} else if cfg!(windows) {
    23
} else if BSD {
    28
} else {
    10
};

fn head(family: u8, len: u8) -> [u8; 2] {
    if BSD {
        [len, family]
    } else {
        (family as u16).to_ne_bytes()
    }
}

/// The bytes of a `sockaddr_in` or `sockaddr_in6` for `addr`.
pub fn encode(addr: SocketAddr) -> Vec<u8> {
    let port = addr.port().to_be_bytes();
    match addr.ip() {
        IpAddr::V4(ip) => {
            let mut out = head(AF_INET, 16).to_vec();
            out.extend_from_slice(&port);
            out.extend_from_slice(&ip.octets());
            out.extend_from_slice(&[0; 8]);
            out
        }
        IpAddr::V6(ip) => {
            let mut out = head(AF_INET6, 28).to_vec();
            out.extend_from_slice(&port);
            out.extend_from_slice(&[0; 4]);
            out.extend_from_slice(&ip.octets());
            out.extend_from_slice(&[0; 4]);
            out
        }
    }
}

/// `ip:port` from the bytes `srt_accept` filled, or `unknown`.
pub fn decode(raw: &[u8]) -> String {
    if raw.len() < 8 {
        return "unknown".into();
    }
    let family = if BSD { raw[1] } else { u16::from_ne_bytes([raw[0], raw[1]]) as u8 };
    let port = u16::from_be_bytes([raw[2], raw[3]]);
    if family == AF_INET {
        let ip = Ipv4Addr::new(raw[4], raw[5], raw[6], raw[7]);
        return SocketAddr::from((ip, port)).to_string();
    }
    if family == AF_INET6 && raw.len() >= 24 {
        let octets: [u8; 16] = raw[8..24].try_into().unwrap_or([0; 16]);
        return SocketAddr::from((Ipv6Addr::from(octets), port)).to_string();
    }
    "unknown".into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_address_comes_back_as_it_went_in() {
        for text in ["127.0.0.1:19382", "10.0.0.5:9000", "[::1]:9000"] {
            let addr: SocketAddr = text.parse().unwrap();
            assert_eq!(decode(&encode(addr)), text);
        }
        assert_eq!(encode("0.0.0.0:9000".parse().unwrap()).len(), 16);
        assert_eq!(decode(&[0; 3]), "unknown");
    }
}
