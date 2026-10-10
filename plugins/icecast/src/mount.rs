//! Feeding an Icecast mount, without libshout.
//!
//! A source client is one HTTP request: `PUT /mount` with a Basic login and the
//! stream's content type, then the encoded sound for as long as it plays. That
//! is all `shout2send` did for us, and the official GStreamer for Windows has
//! no `shout2send`, so Icecast was the one output a Windows install could not
//! use. This is the same request on every platform.
//!
//! The encoder's `appsink` is drained on a thread of its own that holds the
//! connection. A mount that drops the connection, or a server that is not up
//! yet, is dialled again with a backoff, and what arrives meanwhile is dropped:
//! a listener hears a gap, the programme does not wait.
//!
//! A pulled cable sends no FIN and no RST, so the socket has timeouts on the
//! connect and on every write: a write that cannot finish in `STALL` is a
//! connection gone, and the mount is dialled again like any other drop.

use std::io::Write;
use std::net::TcpStream;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use godwinmix_sdk::plugin::Reporter;
use gstreamer as gst;
use gstreamer::prelude::*;
use gstreamer_app as gst_app;

use crate::settings::{Format, Settings};

#[path = "mount_http.rs"]
mod http;
use http::dial;

#[path = "mount_told.rs"]
mod told;
use told::Announced;

/// How long one write may block before the connection counts as gone. The
/// appsink in front drops what arrives meanwhile, so the encoder never waits.
pub const STALL: Duration = Duration::from_secs(10);
/// The backoff between dials, doubling to this.
const MOST_BETWEEN_DIALS: Duration = Duration::from_secs(15);

/// What the sender thread says about itself, for health.
#[derive(Default)]
pub struct State {
    pub connected: AtomicBool,
    pub sent: AtomicU64,
    pub last_error: Mutex<Option<String>>,
}

pub fn content_type(format: Format) -> &'static str {
    match format {
        Format::Mp3 => "audio/mpeg",
        Format::OggVorbis | Format::OggOpus => "application/ogg",
    }
}

/// Start draining `sink` into the mount. Ends when `stop` is set or the sink
/// reaches its end.
pub fn spawn(s: &Settings, sink: gst_app::AppSink, stop: Arc<AtomicBool>, state: Arc<State>, told: Option<Reporter>) -> std::thread::JoinHandle<()> {
    let s = s.clone();
    std::thread::Builder::new()
        .name("gmx-icecast-send".into())
        .spawn(move || {
            let state = Announced { state: &state, told };
            run(&s, &sink, &stop, &state)
        })
        .expect("could not start the Icecast sender")
}

fn run(s: &Settings, sink: &gst_app::AppSink, stop: &AtomicBool, state: &Announced) {
    let mut conn: Option<TcpStream> = None;
    let mut wait = Duration::from_millis(500);
    let mut next_dial = std::time::Instant::now();
    while !stop.load(Ordering::Relaxed) {
        let Some(sample) = sink.try_pull_sample(gst::ClockTime::from_mseconds(100)) else {
            // `is_eos` is also true for an appsink that is not running yet
            // or any more, so it is believed only while the sink plays.
            let playing = sink.current_state() >= gst::State::Paused;
            if sink.is_eos() && playing {
                break;
            }
            if !playing {
                // A stopped appsink answers at once; do not spin on it.
                std::thread::sleep(Duration::from_millis(20));
            }
            continue;
        };
        if conn.is_none() && std::time::Instant::now() >= next_dial {
            match dial(s) {
                Ok(c) => {
                    conn = Some(c);
                    wait = Duration::from_millis(500);
                    state.up();
                }
                Err(e) => {
                    state.down(e);
                    next_dial = std::time::Instant::now() + wait;
                    wait = (wait * 2).min(MOST_BETWEEN_DIALS);
                }
            }
        }
        let (Some(c), Some(buffer)) = (conn.as_mut(), sample.buffer()) else { continue };
        let Ok(map) = buffer.map_readable() else { continue };
        if let Err(e) = c.write_all(map.as_slice()) {
            state.down(match e.kind() {
                std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut => format!(
                    "the server stopped taking the sound for {} s; dialling it again.",
                    STALL.as_secs()
                ),
                _ => format!("the server closed the connection: {e}."),
            });
            conn = None;
            next_dial = std::time::Instant::now() + wait;
            continue;
        }
        state.sent.fetch_add(map.size() as u64, Ordering::Relaxed);
    }
    // A close with a FIN, so the server keeps the last of the sound; Windows
    // drops what a peer has not read yet when a socket is reset.
    if let Some(c) = conn {
        let _ = c.shutdown(std::net::Shutdown::Write);
    }
    state.connected.store(false, Ordering::Relaxed);
}
