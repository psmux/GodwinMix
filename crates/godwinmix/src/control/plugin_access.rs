//! What a plugin is given to reach the core with: the address of `/rpc` in
//! `GMX_RPC` and a token of its own in `GMX_TOKEN`.
//!
//! Both have to be settled before the first plugin starts, which is during
//! `mix.start`, well before the control server is built and binds. So the
//! address is worked out from the bind it was given, and the tokens the loader
//! mints go into a list the control server is later made to share. Before
//! this the address was never set at all, and every plugin saw an empty
//! `GMX_RPC`.

use godwinmix_core::plugin::loader;
use godwinmix_protocol::scope::Tokens;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, ToSocketAddrs};
use std::sync::OnceLock;

static MINTING: OnceLock<Tokens> = OnceLock::new();

/// Mint every plugin instance's token from `tokens`, and remember them so the
/// control server built afterwards accepts what was minted.
pub fn mint_from(tokens: &Tokens) {
    let _ = MINTING.set(tokens.clone());
    let minting = tokens.clone();
    loader::set_token_minter(Box::new(move |plugin, instance| minting.mint_for_plugin(plugin, instance, None)));
}

/// `tokens`, accepting the tokens plugins were given as well.
pub fn accepting_plugins(tokens: Tokens) -> Tokens {
    match MINTING.get() {
        Some(minting) => tokens.sharing_minted(minting),
        None => tokens,
    }
}

/// Tell every plugin started from now on where `/rpc` is. The first address
/// given stays: a process has one control port.
pub fn announce(addr: SocketAddr) {
    loader::set_rpc_url(rpc_url(addr));
}

/// The same from a bind address, when it names a port. A bind on port 0 is
/// left for `announce` once the port is known.
pub fn announce_bind(bind: &str) {
    let addr = bind.parse::<SocketAddr>().ok().or_else(|| bind.to_socket_addrs().ok()?.next());
    if let Some(addr) = addr.filter(|a| a.port() != 0) {
        announce(addr);
    }
}

/// `ws://` and the address a process on this machine reaches `addr` on. A
/// bind on every interface is reached on loopback; the port answers plain
/// HTTP whether or not it answers HTTPS too, so `ws` always works.
pub fn rpc_url(addr: SocketAddr) -> String {
    let ip = match addr.ip() {
        IpAddr::V4(ip) if ip.is_unspecified() => IpAddr::V4(Ipv4Addr::LOCALHOST),
        IpAddr::V6(ip) if ip.is_unspecified() => IpAddr::V6(Ipv6Addr::LOCALHOST),
        ip => ip,
    };
    format!("ws://{}/rpc", SocketAddr::new(ip, addr.port()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_bind_on_every_interface_is_reached_on_loopback() {
        assert_eq!(rpc_url("0.0.0.0:8080".parse().unwrap()), "ws://127.0.0.1:8080/rpc");
        assert_eq!(rpc_url("[::]:8080".parse().unwrap()), "ws://[::1]:8080/rpc");
        assert_eq!(rpc_url("192.168.1.4:9000".parse().unwrap()), "ws://192.168.1.4:9000/rpc");
    }
}
