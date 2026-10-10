//! A destination behind a cable that is pulled and put back. While it is
//! out nothing crosses and nobody sees a FIN or a RST, the way a pulled
//! network cable looks from both ends; the sender has to notice by itself,
//! keep dialling, and be live again once it goes back in, with no one
//! touching it.

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::time::{Duration, Instant};

use godwinmix_protocol::destination::DestinationState;

use super::test_gate::listen;
use super::*;
use crate::media_tag::TagKind;

/// A TCP relay on 127.0.0.1 whose cable can be pulled.
pub struct Cable {
    pub port: u16,
    down: Arc<AtomicBool>,
}

impl Cable {
    pub fn to(target: u16) -> Cable {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let down = Arc::new(AtomicBool::new(false));
        let d = down.clone();
        std::thread::spawn(move || {
            for client in listener.incoming().flatten() {
                let d = d.clone();
                std::thread::spawn(move || {
                    // A connection made while the cable is out is held and
                    // never answered, as a SYN into a dead link would be.
                    while d.load(Ordering::Relaxed) {
                        std::thread::sleep(Duration::from_millis(20));
                    }
                    let Ok(server) = TcpStream::connect(("127.0.0.1", target)) else { return };
                    pipe(client.try_clone().unwrap(), server.try_clone().unwrap(), d.clone());
                    pipe(server, client, d);
                });
            }
        });
        Cable { port, down }
    }

    pub fn pull(&self) {
        self.down.store(true, Ordering::Relaxed);
    }

    pub fn plug(&self) {
        self.down.store(false, Ordering::Relaxed);
    }
}

/// Copy `from` to `to`, holding everything while the cable is out.
fn pipe(mut from: TcpStream, mut to: TcpStream, down: Arc<AtomicBool>) {
    std::thread::spawn(move || {
        let _ = from.set_read_timeout(Some(Duration::from_millis(50)));
        let mut buf = vec![0u8; 64 * 1024];
        loop {
            if down.load(Ordering::Relaxed) {
                std::thread::sleep(Duration::from_millis(20));
                continue;
            }
            match from.read(&mut buf) {
                Ok(0) => return,
                Ok(n) if to.write_all(&buf[..n]).is_err() => return,
                Ok(_) => {}
                Err(e) if super::io::timed_out(&e) => {}
                Err(_) => return,
            }
        }
    });
}

fn tag(kind: TagKind, ts: u32, keyframe: bool, header: bool, body: Vec<u8>) -> MediaTag {
    MediaTag { kind, timestamp_ms: ts, keyframe, sequence_header: header, payload: Arc::from(body) }
}

/// Headers, then a 2 MB/s picture: enough to fill the socket buffers on
/// both sides of the cable within a few seconds of pulling it, which is
/// what a real stream does.
fn heavy_feed(tx: mpsc::Sender<MediaTag>) {
    let _ = tx.send(tag(TagKind::Video, 0, true, true, vec![0x17, 0x00, 0, 0, 0, 0x01, 0x64, 0x00, 0x28]));
    std::thread::spawn(move || {
        for n in 0u32.. {
            let key = n % 10 == 0;
            let mut body = vec![if key { 0x17 } else { 0x27 }, 0x01, 0, 0, 0];
            body.resize(64 * 1024, 0xaa);
            if tx.send(tag(TagKind::Video, 5_000 + n * 33, key, false, body)).is_err() {
                return;
            }
            std::thread::sleep(Duration::from_millis(33));
        }
    });
}

fn wait_for(what: &str, within: Duration, mut ok: impl FnMut() -> bool) -> Duration {
    let started = Instant::now();
    while !ok() {
        assert!(started.elapsed() < within, "timed out after {within:?} waiting for {what}");
        std::thread::sleep(Duration::from_millis(50));
    }
    started.elapsed()
}

#[test]
fn a_pulled_cable_is_noticed_and_the_destination_comes_back_by_itself() {
    let got = Arc::new(Mutex::new(0usize));
    let keep = got.clone();
    let server = listen(0, None, Arc::new(move |t| {
        if t.is_some() {
            *keep.lock().unwrap() += 1;
        }
    }));
    let cable = Cable::to(server.port());
    let (tx, rx) = mpsc::channel();
    heavy_feed(tx);
    let url = format!("rtmp://127.0.0.1:{}/live/behind-a-cable", cable.port);
    let dest = start(Target::new("cable", "custom", &url), rx);
    let state = || dest.stats().live.state;
    wait_for("the first connection", Duration::from_secs(10), || *got.lock().unwrap() > 20);
    assert_eq!(state(), DestinationState::Live);

    cable.pull();
    let noticed = wait_for("the sender to notice", Duration::from_secs(60), || state() != DestinationState::Live);
    // Out for longer than a dial takes to time out, so the retries run into
    // the dead link too and have to keep going.
    std::thread::sleep(Duration::from_secs(25));
    assert_ne!(state(), DestinationState::Failed, "a pulled cable is not a refusal: {:?}", dest.stats().live.error);
    assert_ne!(state(), DestinationState::Off);
    let before = *got.lock().unwrap();
    cable.plug();
    let back = wait_for("live again", Duration::from_secs(30), || {
        state() == DestinationState::Live && *got.lock().unwrap() > before + 20
    });
    eprintln!("noticed the pull in {noticed:?}, live again {back:?} after the cable went back");
    assert!(dest.stats().live.reconnects >= 1);
}
