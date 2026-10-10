//! A TCP relay whose cable can be pulled, in process.
//!
//! The same thing as the `cable.py` used by hand on 2026-10-10: while it is
//! unplugged nothing crosses and nobody sees a FIN or a reset. Connections it
//! is carrying stop moving, and a connection made meanwhile is accepted and
//! then left hanging, which is what a client sees through a dead relay or a
//! server that took the connection and never answered.

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

pub struct Cable {
    pub port: u16,
    plugged: Arc<AtomicBool>,
    accepted: Arc<AtomicUsize>,
}

impl Cable {
    /// A relay to `target` on loopback, plugged in or not.
    pub fn to(target: u16, plugged: bool) -> Self {
        let listener = bind();
        let port = listener.local_addr().unwrap().port();
        let cable = Self {
            port,
            plugged: Arc::new(AtomicBool::new(plugged)),
            accepted: Arc::new(AtomicUsize::new(0)),
        };
        let (on, count) = (cable.plugged.clone(), cable.accepted.clone());
        std::thread::spawn(move || {
            for client in listener.incoming().flatten() {
                count.fetch_add(1, Ordering::SeqCst);
                let on = on.clone();
                std::thread::spawn(move || carry(client, target, &on));
            }
        });
        cable
    }

    pub fn set(&self, plugged: bool) {
        self.plugged.store(plugged, Ordering::SeqCst);
    }

    /// Connections accepted so far, each one a connect attempt.
    pub fn accepted(&self) -> usize {
        self.accepted.load(Ordering::SeqCst)
    }
}

/// The machine's rule is ports 20200 to 20299 for this work; anywhere else
/// (a CI runner) any free port will do.
fn bind() -> TcpListener {
    (20240..20300)
        .find_map(|p| TcpListener::bind(("127.0.0.1", p)).ok())
        .unwrap_or_else(|| TcpListener::bind("127.0.0.1:0").expect("a loopback port"))
}

fn carry(client: TcpStream, target: u16, on: &Arc<AtomicBool>) {
    // Held, not closed, while the cable is out: the client sees nothing.
    while !on.load(Ordering::SeqCst) {
        std::thread::sleep(Duration::from_millis(50));
    }
    let Ok(server) = TcpStream::connect(("127.0.0.1", target)) else { return };
    let (c2, s2) = (client.try_clone().unwrap(), server.try_clone().unwrap());
    let on2 = on.clone();
    std::thread::spawn(move || pump(s2, c2, &on2));
    pump(client, server, on);
}

fn pump(mut from: TcpStream, mut to: TcpStream, on: &Arc<AtomicBool>) {
    let _ = from.set_read_timeout(Some(Duration::from_millis(100)));
    let mut buf = vec![0u8; 64 * 1024];
    loop {
        if !on.load(Ordering::SeqCst) {
            std::thread::sleep(Duration::from_millis(50));
            continue;
        }
        match from.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => {
                if to.write_all(&buf[..n]).is_err() {
                    break;
                }
            }
            Err(e) if matches!(e.kind(), std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut) => {}
            Err(_) => break,
        }
    }
    let _ = to.shutdown(std::net::Shutdown::Both);
}
