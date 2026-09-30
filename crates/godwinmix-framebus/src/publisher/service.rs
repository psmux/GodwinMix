//! The bus's own thread: accept readers, hand them the region, and free a
//! reader's place and leases the moment its socket closes.
//!
//! The streaming thread and this one share two locks, and neither is held
//! across anything that can wait: the reader list only while a reader is
//! added, removed or nudged, and a reader's send lock only for one send. The
//! streaming thread takes the send lock with `try_lock`, so a region being
//! sent to a slow reader costs that reader one nudge and nobody anything.

use std::os::fd::AsRawFd;
use std::os::unix::net::{UnixListener, UnixStream};
use std::sync::atomic::{AtomicBool, Ordering::Relaxed};
use std::sync::{Arc, Mutex};

use super::conn::{poll_all, read_reader, send_region, Conn};
use crate::link::{self, Inbox};
use crate::ring::Ring;
use crate::Error;

pub struct Shared {
    ring: Mutex<Arc<Ring>>,
    readers: Mutex<Vec<Arc<Conn>>>,
    stop: AtomicBool,
    new_region: AtomicBool,
    wake_tx: UnixStream,
    wake_rx: UnixStream,
}

impl Shared {
    pub fn new(ring: Arc<Ring>) -> Result<Shared, Error> {
        let (wake_tx, wake_rx) = UnixStream::pair()?;
        wake_tx.set_nonblocking(true)?;
        wake_rx.set_nonblocking(true)?;
        Ok(Shared {
            ring: Mutex::new(ring),
            readers: Mutex::new(Vec::new()),
            stop: AtomicBool::new(false),
            new_region: AtomicBool::new(false),
            wake_tx,
            wake_rx,
        })
    }

    fn wake(&self) {
        let _ = link::send(self.wake_tx.as_raw_fd(), &[1], None, true);
    }

    pub fn stop(&self) {
        self.stop.store(true, Relaxed);
        self.wake();
    }

    pub fn replace(&self, ring: Arc<Ring>) {
        *self.ring.lock().unwrap() = ring;
        self.new_region.store(true, Relaxed);
        self.wake();
    }

    /// One byte to every reader, never waiting. Called after every frame.
    pub fn nudge(&self) {
        let readers = self.readers.lock().unwrap();
        for c in readers.iter() {
            if let Ok(_g) = c.send.try_lock() {
                let _ = link::send(c.stream.as_raw_fd(), &[link::NUDGE], None, true);
            }
        }
    }

    pub fn run(&self, listener: UnixListener) {
        let mut inboxes: Vec<(Arc<Conn>, Inbox)> = Vec::new();
        while !self.stop.load(Relaxed) {
            let mut fds = vec![self.wake_rx.as_raw_fd(), listener.as_raw_fd()];
            fds.extend(inboxes.iter().map(|(c, _)| c.stream.as_raw_fd()));
            let ready = poll_all(&fds, 500);
            // Readers first, while `ready` still lines up with `inboxes`.
            let mut i = inboxes.len();
            while i > 0 {
                i -= 1;
                if ready[i + 2] && !read_reader(&mut inboxes[i]) {
                    let (c, _) = inboxes.swap_remove(i);
                    self.drop_reader(&c);
                }
            }
            if ready[0] {
                let mut sink = [0u8; 64];
                let _ = std::io::Read::read(&mut &self.wake_rx, &mut sink);
                if self.new_region.swap(false, Relaxed) {
                    self.send_region_to_all(&mut inboxes);
                }
            }
            if ready[1] {
                while let Ok((stream, _)) = listener.accept() {
                    if let Some(c) = self.admit(stream) {
                        inboxes.push((c, Inbox::default()));
                    }
                }
            }
        }
        for (c, _) in inboxes {
            self.drop_reader(&c);
        }
    }

    fn admit(&self, stream: UnixStream) -> Option<Arc<Conn>> {
        let ring = self.ring.lock().unwrap().clone();
        let reader = ring.take_reader(0)?;
        let conn = Arc::new(Conn {
            stream,
            send: Mutex::new(()),
            reader: Mutex::new(reader),
            pid: Default::default(),
        });
        if send_region(&conn, &ring, reader).is_err() {
            ring.reset_reader(reader);
            return None;
        }
        conn.stream.set_nonblocking(true).ok()?;
        self.readers.lock().unwrap().push(conn.clone());
        Some(conn)
    }

    fn send_region_to_all(&self, inboxes: &mut Vec<(Arc<Conn>, Inbox)>) {
        let ring = self.ring.lock().unwrap().clone();
        inboxes.retain(|(c, _)| {
            let ok = ring.take_reader(c.pid.load(Relaxed)).is_some_and(|r| {
                *c.reader.lock().unwrap() = r;
                send_region(c, &ring, r).is_ok()
            });
            if !ok {
                self.readers.lock().unwrap().retain(|x| !Arc::ptr_eq(x, c));
            }
            ok
        });
    }

    fn drop_reader(&self, c: &Arc<Conn>) {
        self.readers.lock().unwrap().retain(|x| !Arc::ptr_eq(x, c));
        self.ring.lock().unwrap().reset_reader(*c.reader.lock().unwrap());
    }
}
