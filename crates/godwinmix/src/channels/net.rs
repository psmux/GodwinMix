//! The addresses an encoder is told to publish to.

use std::net::{Ipv4Addr, SocketAddr, UdpSocket};

/// This machine's address on the network it would reach the internet by, or
/// the loopback when it has none.
///
/// Worked out by asking the routing table where a packet to a public address
/// would leave from: no interface enumeration and no crate. Connecting a UDP
/// socket sends nothing. Not a documentation address, as the node discovery
/// uses: a VPN client that numbers its tunnel 192.0.2.1 answers that probe
/// with the tunnel, which no encoder on the LAN can reach.
pub fn first_address() -> String {
    let found = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0))
        .and_then(|s| s.connect((Ipv4Addr::new(1, 1, 1, 1), 9)).map(|_| s))
        .and_then(|s| s.local_addr());
    match found {
        Ok(SocketAddr::V4(v4)) if !v4.ip().is_loopback() && !v4.ip().is_unspecified() => v4.ip().to_string(),
        _ => "127.0.0.1".into(),
    }
}

/// `rtmp://<address>:<port>` for each address worth offering.
pub fn urls(port: u16) -> Vec<String> {
    let mut out = vec![format!("rtmp://{}:{port}", first_address())];
    let local = format!("rtmp://127.0.0.1:{port}");
    if !out.contains(&local) {
        out.push(local);
    }
    out
}

/// Why nothing is listening, and what to do about it.
pub fn why_not_listening(plugin: &str) -> String {
    use godwinmix_core::plugin::loader;
    if loader::get(plugin).is_none() {
        return format!(
            "nothing is listening for RTMP: the {plugin} plugin is not installed. Install it \
             from the Plugins page and channels start taking publishers."
        );
    }
    if !loader::enabled().iter().any(|p| p.name() == plugin) {
        return format!(
            "nothing is listening for RTMP: the {plugin} plugin is switched off. Switch it on \
             from the Plugins page."
        );
    }
    format!(
        "the {plugin} plugin is installed but its RTMP listener is not running. Another \
         program may hold the port; the plugin's log line says which."
    )
}
