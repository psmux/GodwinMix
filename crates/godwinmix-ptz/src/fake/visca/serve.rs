//! The fake VISCA camera's two sockets: UDP with Sony's header, and raw TCP.

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, UdpSocket};

use super::{handle, Options};
use crate::fake::Shared;
use crate::visca::ip;

/// A VISCA over IP camera on a loopback UDP port. Answers its address.
pub fn serve_ip(head: Shared, options: Options) -> SocketAddr {
    let socket = UdpSocket::bind("127.0.0.1:0").expect("a loopback port");
    let at = socket.local_addr().expect("bound");
    std::thread::spawn(move || {
        let (mut buf, mut seen) = ([0u8; 512], 0u32);
        let mut reset = !options.want_reset;
        while let Ok((n, from)) = socket.recv_from(&mut buf) {
            seen += 1;
            if options.drop_every > 0 && seen % options.drop_every == 0 {
                continue;
            }
            let Some((kind, seq, payload)) = ip::unwrap(&buf[..n]) else { continue };
            let replies = if kind == ip::CONTROL && payload == ip::RESET {
                reset = true;
                vec![ip::wrap(ip::CONTROL_REPLY, seq, &[0x01])]
            } else if !reset {
                vec![ip::wrap(ip::CONTROL_REPLY, seq, &ip::SEQ_ABNORMAL)]
            } else {
                let mut h = head.lock().unwrap();
                handle(&mut h, payload).into_iter().map(|r| ip::wrap(ip::REPLY, seq, &r)).collect()
            };
            for r in replies {
                let _ = socket.send_to(&r, from);
            }
        }
    });
    at
}

/// A raw VISCA camera on a loopback TCP port.
pub fn serve_tcp(head: Shared) -> SocketAddr {
    let listener = TcpListener::bind("127.0.0.1:0").expect("a loopback port");
    let at = listener.local_addr().expect("bound");
    std::thread::spawn(move || {
        for conn in listener.incoming().flatten() {
            let head = head.clone();
            std::thread::spawn(move || {
                let (mut conn, mut pending, mut buf) = (conn, Vec::new(), [0u8; 256]);
                while let Ok(n) = conn.read(&mut buf) {
                    if n == 0 {
                        break;
                    }
                    pending.extend_from_slice(&buf[..n]);
                    while let Some(end) = pending.iter().position(|b| *b == 0xFF) {
                        let m: Vec<u8> = pending.drain(..=end).collect();
                        let replies = handle(&mut head.lock().unwrap(), &m);
                        for r in replies {
                            let _ = conn.write_all(&r);
                        }
                    }
                }
            });
        }
    });
    at
}
