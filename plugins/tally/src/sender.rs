//! Getting bytes to the lamps, over UDP or over TCP.
//!
//! UDP is one datagram per packet and never fails in a way worth retrying: a
//! lamp that missed one gets the next one, and the refresh timer means it is
//! never more than a few seconds behind. TCP is a stream that has to be
//! reconnected, so the sender holds the connection and re-opens it on the next
//! send after a failure rather than spinning on a socket nobody is listening
//! on.

use tokio::io::AsyncWriteExt;
use tokio::net::{TcpStream, UdpSocket};

use crate::settings::Protocol;

/// Where the packets go.
pub enum Sender {
    Udp {
        socket: UdpSocket,
        address: String,
    },
    Tcp {
        address: String,
        stream: Option<TcpStream>,
    },
}

impl Sender {
    pub async fn open(protocol: Protocol, address: &str) -> std::io::Result<Sender> {
        match protocol {
            Protocol::Udp => Ok(Sender::Udp {
                socket: UdpSocket::bind("0.0.0.0:0").await?,
                address: address.to_string(),
            }),
            // Not connected yet on purpose: a tally interface that is powered
            // on after the mixer must not stop the mixer from starting.
            Protocol::Tcp => Ok(Sender::Tcp {
                address: address.to_string(),
                stream: None,
            }),
        }
    }

    /// Send one packet. Returns the error rather than logging it, so the caller
    /// decides how loud to be about a lamp that is switched off.
    pub async fn send(&mut self, packet: &[u8]) -> std::io::Result<()> {
        match self {
            Sender::Udp { socket, address } => {
                socket.send_to(packet, address.as_str()).await?;
                Ok(())
            }
            Sender::Tcp { address, stream } => {
                if stream.is_none() {
                    *stream = Some(TcpStream::connect(address.as_str()).await?);
                }
                let Some(open) = stream.as_mut() else {
                    unreachable!("just opened");
                };
                match open.write_all(packet).await {
                    Ok(()) => Ok(()),
                    Err(error) => {
                        // The next send re-opens. TSL over TCP carries the byte
                        // count in every packet, so a reconnect loses at most
                        // the packet that failed and never desynchronises the
                        // receiver.
                        *stream = None;
                        Err(error)
                    }
                }
            }
        }
    }

    pub fn describe(&self) -> String {
        match self {
            Sender::Udp { address, .. } => format!("udp {address}"),
            Sender::Tcp { address, .. } => format!("tcp {address}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tsl;

    #[tokio::test]
    async fn a_udp_packet_arrives_as_it_was_written() {
        let listener = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap().to_string();
        let mut sender = Sender::open(Protocol::Udp, &address).await.unwrap();
        assert!(sender.describe().starts_with("udp "));

        let display = tsl::Display {
            index: 2,
            right: tsl::Lamp::Red,
            label: "CAM 3".into(),
            ..tsl::Display::default()
        };
        sender.send(&tsl::encode(0, &display, false)).await.unwrap();

        let mut buffer = [0u8; 256];
        let (length, _) = listener.recv_from(&mut buffer).await.unwrap();
        let read = tsl::decode(&buffer[..length]).expect("a packet");
        assert_eq!(read.displays, vec![display]);
    }

    #[tokio::test]
    async fn a_tcp_sender_connects_on_the_first_packet_and_not_before() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap().to_string();
        let mut sender = Sender::open(Protocol::Tcp, &address).await.unwrap();
        assert!(matches!(sender, Sender::Tcp { stream: None, .. }), "not yet connected");

        let display = tsl::Display {
            index: 1,
            right: tsl::Lamp::Green,
            label: "CAM 2".into(),
            ..tsl::Display::default()
        };
        let accept = tokio::spawn(async move { listener.accept().await });
        sender.send(&tsl::encode(0, &display, false)).await.unwrap();
        let (mut stream, _) = accept.await.unwrap().unwrap();

        let mut buffer = [0u8; 256];
        let length = tokio::io::AsyncReadExt::read(&mut stream, &mut buffer).await.unwrap();
        let read = tsl::decode(&buffer[..length]).expect("a packet");
        assert_eq!(read.displays[0].label, "CAM 2");
        assert!(matches!(sender, Sender::Tcp { stream: Some(_), .. }), "now connected");
    }

    #[tokio::test]
    async fn a_tcp_sender_with_nothing_listening_answers_with_the_error() {
        // Port 1 on loopback: nothing binds it, and connecting fails at once.
        let mut sender = Sender::open(Protocol::Tcp, "127.0.0.1:1").await.unwrap();
        assert!(sender.send(b"x").await.is_err());
        assert!(matches!(sender, Sender::Tcp { stream: None, .. }));
    }
}
