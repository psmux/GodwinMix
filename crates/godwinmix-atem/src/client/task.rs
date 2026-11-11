//! The loop that joins a session to a socket.

use std::net::SocketAddr;
use std::time::Instant;

use tokio::net::UdpSocket;
use tokio::sync::{mpsc, watch};

use super::{Reply, Snapshot};
use crate::command::Command;
use crate::session::{Link, Session};

/// How long to wait before trying a socket or a name lookup again.
const RETRY: std::time::Duration = std::time::Duration::from_secs(2);

pub(super) async fn run(
    address: String,
    publish: watch::Sender<Snapshot>,
    mut inbox: mpsc::UnboundedReceiver<(Vec<Command>, Reply)>,
) {
    loop {
        let (socket, peer) = match open(&address).await {
            Ok(pair) => pair,
            Err(reason) => {
                publish.send_modify(|s| s.link = Link { reason: Some(reason), ..Link::connecting() });
                tokio::time::sleep(RETRY).await;
                continue;
            }
        };
        if serve(&socket, peer, &publish, &mut inbox).await.is_none() {
            return;
        }
    }
}

/// Resolve the address and bind a socket for it.
async fn open(address: &str) -> Result<(UdpSocket, SocketAddr), String> {
    let target = with_port(address);
    let peer = tokio::net::lookup_host(&target)
        .await
        .map_err(|e| format!("cannot resolve {address}: {e}. Check the name or use the IP address"))?
        .next()
        .ok_or_else(|| format!("{address} resolved to no address. Use the IP address"))?;
    let local: SocketAddr = if peer.is_ipv4() { "0.0.0.0:0" } else { "[::]:0" }.parse().unwrap();
    let socket = UdpSocket::bind(local)
        .await
        .map_err(|e| format!("cannot open a UDP socket: {e}"))?;
    Ok((socket, peer))
}

/// Run one socket's worth. `None` when the handle has gone and the task
/// should end.
async fn serve(
    socket: &UdpSocket,
    peer: SocketAddr,
    publish: &watch::Sender<Snapshot>,
    inbox: &mut mpsc::UnboundedReceiver<(Vec<Command>, Reply)>,
) -> Option<()> {
    let mut session = Session::new(seed(), Instant::now());
    let mut buffer = vec![0u8; 2048];
    loop {
        while let Some(datagram) = session.poll_transmit() {
            // A failed send is a lost packet, which the protocol already
            // survives; the silence timer catches a link that stays down.
            let _ = socket.send_to(&datagram, peer).await;
        }
        if session.take_changed() {
            publish.send_modify(|s| {
                s.link = session.link().clone();
                s.state = session.state().clone();
            });
        }
        let deadline = tokio::time::Instant::from_std(session.poll_timeout());
        tokio::select! {
            received = socket.recv_from(&mut buffer) => {
                if let Ok((length, from)) = received {
                    if from == peer {
                        session.handle_datagram(&buffer[..length], Instant::now());
                    }
                }
            }
            _ = tokio::time::sleep_until(deadline) => session.handle_timeout(Instant::now()),
            request = inbox.recv() => {
                let (commands, reply) = request?;
                let _ = reply.send(session.send(&commands, Instant::now()));
            }
        }
    }
}

/// The address with port 9910 added when it has none.
pub(crate) fn with_port(address: &str) -> String {
    let address = address.trim();
    if address.parse::<SocketAddr>().is_ok() {
        return address.to_string();
    }
    if let Ok(ip) = address.parse::<std::net::IpAddr>() {
        return SocketAddr::new(ip, crate::PORT).to_string();
    }
    match address.rsplit_once(':') {
        Some((host, port)) if !host.contains(':') && port.parse::<u16>().is_ok() => address.to_string(),
        _ => format!("{address}:{}", crate::PORT),
    }
}

#[cfg(test)]
#[test]
fn a_port_is_added_only_when_missing() {
    assert_eq!(with_port("192.0.2.10"), "192.0.2.10:9910");
    assert_eq!(with_port("192.0.2.10:9911"), "192.0.2.10:9911");
    assert_eq!(with_port("atem.example.com"), "atem.example.com:9910");
    assert_eq!(with_port("atem.example.com:9000"), "atem.example.com:9000");
    assert_eq!(with_port("2001:db8::1"), "[2001:db8::1]:9910");
}

/// A session id for the hello. Any value works; differing ones keep two
/// clients on one machine apart while their hellos are in flight.
fn seed() -> u16 {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.subsec_nanos())
        .unwrap_or(0x53ab);
    ((nanos ^ std::process::id()) & 0x7fff) as u16
}
