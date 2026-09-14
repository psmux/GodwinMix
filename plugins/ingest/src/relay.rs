//! Handing one publisher's stream to one source process.
//!
//! Only one process can hold port 1935. When `ingest/discover` holds it so that
//! many publishers can arrive on one address, each `ingest/rtmp` source has to
//! get its bytes from somewhere else. That somewhere is a loopback TCP port per
//! publisher: the device opens one, puts its address in the candidate it
//! reports, and the source connects and copies bytes to the core.
//!
//! It is deliberately the dumbest thing that works. No framing, no protocol, no
//! authentication beyond binding to 127.0.0.1: the payload is already a
//! self describing FLV stream and both ends are on this machine.

use std::io::Write;
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

/// A loopback fan-out for one publisher's FLV stream.
pub struct Relay {
    port: u16,
    shared: Arc<Shared>,
    stop: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}

struct Shared {
    clients: Mutex<Vec<TcpStream>>,
    /// What a reader that joins late is sent first: the FLV file header, then
    /// the codec sequence headers. Without these a decoder has nothing to
    /// decode the first frames against.
    preamble: Mutex<Vec<u8>>,
}

impl Relay {
    /// Open a relay on an operating system chosen loopback port.
    pub fn open() -> Result<Relay, String> {
        let listener = TcpListener::bind(("127.0.0.1", 0))
            .map_err(|e| format!("could not open a relay port on the loopback: {e}"))?;
        let port = listener
            .local_addr()
            .map(|a| a.port())
            .map_err(|e| format!("the relay has no address: {e}"))?;
        let shared = Arc::new(Shared {
            clients: Mutex::new(Vec::new()),
            preamble: Mutex::new(Vec::new()),
        });
        let stop = Arc::new(AtomicBool::new(false));
        let thread = spawn_accept(listener, Arc::clone(&shared), Arc::clone(&stop));
        Ok(Relay { port, shared, stop, thread: Some(thread) })
    }

    /// The address to put in a candidate's params.
    pub fn address(&self) -> String {
        format!("127.0.0.1:{}", self.port)
    }

    #[cfg(test)]
    pub fn port(&self) -> u16 {
        self.port
    }

    /// Keep these bytes for a reader that joins later. Called with the FLV
    /// header and with each codec sequence header, and with nothing else.
    pub fn remember(&self, bytes: &[u8]) {
        let mut held = self.shared.preamble.lock().unwrap_or_else(|e| e.into_inner());
        held.extend_from_slice(bytes);
    }

    /// Send to every connected reader, dropping the ones that have gone.
    ///
    /// A reader that is not keeping up gets dropped rather than slowing the
    /// publisher down: the rule is that nothing a plugin does may stall the
    /// media path, and a source process that has stopped reading is a source
    /// process that has died.
    pub fn send(&self, bytes: &[u8]) {
        let mut clients = self.shared.clients.lock().unwrap_or_else(|e| e.into_inner());
        clients.retain_mut(|client| client.write_all(bytes).is_ok());
    }

    /// How many readers are connected.
    #[cfg(test)]
    pub fn readers(&self) -> usize {
        self.shared.clients.lock().unwrap_or_else(|e| e.into_inner()).len()
    }
}

impl Drop for Relay {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        let _ = TcpStream::connect(("127.0.0.1", self.port));
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn spawn_accept(
    listener: TcpListener,
    shared: Arc<Shared>,
    stop: Arc<AtomicBool>,
) -> std::thread::JoinHandle<()> {
    std::thread::Builder::new()
        .name("gmx-relay-accept".into())
        .spawn(move || {
            for incoming in listener.incoming() {
                if stop.load(Ordering::Relaxed) {
                    return;
                }
                let Ok(mut client) = incoming else { continue };
                let preamble = shared.preamble.lock().unwrap_or_else(|e| e.into_inner()).clone();
                if !preamble.is_empty() && client.write_all(&preamble).is_err() {
                    continue;
                }
                shared
                    .clients
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .push(client);
            }
        })
        .expect("could not start the relay accept thread")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;

    fn wait_for_readers(relay: &Relay, want: usize) {
        for _ in 0..50 {
            if relay.readers() >= want {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
    }

    #[test]
    fn a_relay_gives_an_address_on_the_loopback_only() {
        let relay = Relay::open().expect("the loopback has a free port");
        assert!(relay.address().starts_with("127.0.0.1:"));
        assert!(relay.port() > 0);
        assert_eq!(relay.readers(), 0);
    }

    #[test]
    fn a_reader_gets_the_preamble_first_and_then_the_stream() {
        let relay = Relay::open().expect("open");
        relay.remember(b"FLVHEADER");
        let mut reader = TcpStream::connect(relay.address()).expect("connect");
        wait_for_readers(&relay, 1);
        relay.send(b"TAGS");

        let mut got = [0u8; 13];
        reader.set_read_timeout(Some(std::time::Duration::from_secs(2))).ok();
        let mut read = 0;
        while read < got.len() {
            match reader.read(&mut got[read..]) {
                Ok(0) => break,
                Ok(n) => read += n,
                Err(_) => break,
            }
        }
        assert_eq!(&got[..read], b"FLVHEADERTAGS");
    }

    #[test]
    fn a_reader_that_went_away_is_dropped_rather_than_blocking_the_publisher() {
        let relay = Relay::open().expect("open");
        let reader = TcpStream::connect(relay.address()).expect("connect");
        wait_for_readers(&relay, 1);
        drop(reader);
        // Enough traffic that the socket buffer cannot swallow it all.
        for _ in 0..200 {
            relay.send(&[0u8; 4096]);
        }
        assert_eq!(relay.readers(), 0);
    }

    #[test]
    fn two_readers_both_get_the_stream() {
        let relay = Relay::open().expect("open");
        relay.remember(b"H");
        let mut one = TcpStream::connect(relay.address()).expect("connect");
        let mut two = TcpStream::connect(relay.address()).expect("connect");
        wait_for_readers(&relay, 2);
        relay.send(b"X");
        for reader in [&mut one, &mut two] {
            let mut got = [0u8; 2];
            reader.set_read_timeout(Some(std::time::Duration::from_secs(2))).ok();
            let mut read = 0;
            while read < got.len() {
                match reader.read(&mut got[read..]) {
                    Ok(0) => break,
                    Ok(n) => read += n,
                    Err(_) => break,
                }
            }
            assert_eq!(&got[..read], b"HX");
        }
    }
}
