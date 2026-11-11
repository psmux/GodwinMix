//! A fake ATEM on loopback: answers the hello, sends a state dump as
//! acknowledged packets and resends what is not acknowledged, acks the
//! client's packets in order, keeps every command block it took, and answers
//! each with the state blocks a switcher would send. It can lose one
//! datagram in `n` at random, both ways, to prove the client gets there anyway.

#![allow(dead_code)]

pub mod dump;
pub mod sim;
mod wire;

use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use godwinmix_atem::wire::{Flags, Header, HEADER_LEN};
use tokio::net::UdpSocket;

/// What the fake took: each command block whole, and each packet header.
#[derive(Default)]
pub struct Seen {
    pub blocks: Vec<Vec<u8>>,
    pub headers: Vec<Header>,
    pub hellos: u32,
    pub sim: sim::Sim,
}

pub struct Fake {
    pub addr: SocketAddr,
    pub seen: Arc<Mutex<Seen>>,
    task: tokio::task::JoinHandle<()>,
}

pub(crate) struct Wire {
    peer: Option<SocketAddr>,
    session: u16,
    next_id: u16,
    expect: u16,
    inflight: Vec<(u16, Vec<u8>, Instant)>,
    count: u32,
    lose_every: u32,
}

impl Fake {
    /// Listen on `port` (0 for any), losing one datagram in `lose_every`
    /// (0 for none).
    pub async fn start(port: u16, lose_every: u32) -> Fake {
        let socket = UdpSocket::bind(("127.0.0.1", port)).await.expect("bind the fake");
        let addr = socket.local_addr().unwrap();
        let seen = Arc::new(Mutex::new(Seen::default()));
        let task = tokio::spawn(run(socket, seen.clone(), lose_every));
        Fake { addr, seen, task }
    }

    /// Stop answering and let go of the port.
    pub async fn stop(self) {
        self.task.abort();
        let _ = self.task.await;
    }

    pub fn blocks(&self) -> Vec<Vec<u8>> {
        self.seen.lock().unwrap().blocks.clone()
    }

    /// Wait until the fake has taken `n` command blocks.
    pub async fn wait_blocks(&self, n: usize) -> Vec<Vec<u8>> {
        let deadline = Instant::now() + Duration::from_secs(10);
        while self.blocks().len() < n {
            assert!(Instant::now() < deadline, "the fake took {} of {n} blocks", self.blocks().len());
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        self.blocks()
    }
}

async fn run(socket: UdpSocket, seen: Arc<Mutex<Seen>>, lose_every: u32) {
    let mut w = Wire { peer: None, session: 0x8a01, next_id: 1, expect: 1, inflight: Vec::new(), count: 0x2545_f491, lose_every };
    let mut buf = [0u8; 2048];
    let mut tick = tokio::time::interval(Duration::from_millis(10));
    loop {
        tokio::select! {
            got = socket.recv_from(&mut buf) => {
                let Ok((n, from)) = got else { continue };
                if w.lose() { continue; }
                w.peer = Some(from);
                let Some(h) = Header::parse(&buf[..n]) else { continue };
                for out in w.take(&h, &buf[HEADER_LEN..n], &seen) {
                    w.send(&socket, out).await;
                }
            }
            _ = tick.tick() => {
                let due: Vec<Vec<u8>> = w.inflight.iter_mut()
                    .filter(|p| p.2.elapsed() >= Duration::from_millis(40))
                    .map(|p| { p.2 = Instant::now(); p.1[0] |= Flags::RETRANSMIT.0 << 3; p.1.clone() })
                    .collect();
                for out in due { w.send(&socket, out).await; }
            }
        }
    }
}
