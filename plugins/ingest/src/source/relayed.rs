//! An `ingest/rtmp` source that reads one stream from the channel server.
//!
//! It asks the hub for `<channel>/<stream>` over loopback and copies what
//! comes back into the remuxer. If the stream is not live yet it waits,
//! quietly, for as long as it takes, and if the channel server itself is not
//! up yet (the mixer restores its sources before it starts its plugins) it
//! asks again once a second.
//!
//! When the publisher leaves, the hub ends the stream and this process exits.
//! That is on purpose. What went down stdout was one Matroska stream with one
//! set of codec headers and one timeline, and the next publisher's would be a
//! second one spliced onto the end of it, which no demuxer accepts. The core
//! restarts a source that ends in place, behind its freeze frame (held for up
//! to 45 s, then the slate), and the new process waits for the next publisher
//! on a clean pipe.

use std::io::{ErrorKind, Read};
use std::net::TcpStream;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use godwinmix_sdk::plugin::Reporter;
use godwinmix_sdk::wire::Health;

use super::{Settings, State};
use crate::remux::Remux;

pub fn start(
    settings: &Settings,
    reporter: Option<Reporter>,
    remux: Remux,
    state: Arc<State>,
    stop: Arc<AtomicBool>,
    exit_at_end: bool,
) -> Result<std::thread::JoinHandle<()>, String> {
    let (relay, stream) = (settings.relay.clone(), settings.stream.clone());
    state.set_address(format!("the channel server's {stream}"));
    std::thread::Builder::new()
        .name("gmx-ingest-relay".into())
        .spawn(move || {
            let Some(socket) = connect(&relay, &stream, &reporter, &stop) else { return };
            copy(socket, &stream, &reporter, &remux, &state, &stop);
            state.set_publisher(None);
            if stop.load(Ordering::Relaxed) || !exit_at_end {
                return;
            }
            if let Some(r) = &reporter {
                r.info(format!("{stream} stopped; ending this source so the next publisher starts clean"));
            }
            // End of stream through the muxer, so what was sent is whole.
            drop(remux);
            std::process::exit(0);
        })
        .map_err(|e| format!("could not start the relay reader: {e}"))
}

/// Reach the channel server, asking again once a second until it answers or
/// this source is stopped.
fn connect(relay: &str, stream: &str, reporter: &Option<Reporter>, stop: &AtomicBool) -> Option<TcpStream> {
    let mut told = false;
    while !stop.load(Ordering::Relaxed) {
        match crate::relay::request(relay, stream) {
            Ok(socket) => {
                let _ = socket.set_read_timeout(Some(Duration::from_secs(1)));
                if let Some(r) = reporter {
                    r.health_changed(Health::degraded(format!("waiting for {stream} to go live")));
                }
                return Some(socket);
            }
            Err(why) => {
                if let (false, Some(r)) = (told, reporter) {
                    r.health_changed(Health::degraded(why));
                    told = true;
                }
                std::thread::sleep(Duration::from_secs(1));
            }
        }
    }
    None
}

/// Copy the stream into the remuxer until it ends.
fn copy(
    mut socket: TcpStream,
    stream: &str,
    reporter: &Option<Reporter>,
    remux: &Remux,
    state: &State,
    stop: &AtomicBool,
) {
    let mut buffer = vec![0u8; 64 * 1024];
    let mut live = false;
    while !stop.load(Ordering::Relaxed) {
        let n = match socket.read(&mut buffer) {
            Ok(0) => break,
            Ok(n) => n,
            // A second without bytes is a wait, not an end: it is how this
            // thread notices it has been asked to stop.
            Err(e) if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => continue,
            Err(_) => break,
        };
        // Thirteen bytes of FLV header arrive at once; the stream is live when
        // the first tag does.
        if !live && state.bytes.load(Ordering::Relaxed) + n as u64 > 13 {
            live = true;
            state.set_publisher(Some(stream.to_string()));
            if let Some(r) = reporter {
                let mut health = Health::ok();
                health.detail = Some(format!("{stream} is live"));
                r.health_changed(health);
            }
        }
        remux.write(&buffer[..n]);
        state.wrote(n, remux);
        if remux.broken() {
            break;
        }
    }
}
