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

use std::io::Write;
use std::net::TcpStream;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use gstreamer as gst;
use gstreamer::prelude::*;
use gstreamer_app as gst_app;

use crate::settings::{Format, Settings};

#[path = "mount_http.rs"]
mod http;
use http::{read_answer, request};

/// How long a server gets to answer the request before it is called down.
const ANSWER_WITHIN: Duration = Duration::from_secs(10);
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
pub fn spawn(s: &Settings, sink: gst_app::AppSink, stop: Arc<AtomicBool>, state: Arc<State>) -> std::thread::JoinHandle<()> {
    let s = s.clone();
    std::thread::Builder::new()
        .name("gmx-icecast-send".into())
        .spawn(move || run(&s, &sink, &stop, &state))
        .expect("could not start the Icecast sender")
}

fn run(s: &Settings, sink: &gst_app::AppSink, stop: &AtomicBool, state: &State) {
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
                    state.connected.store(true, Ordering::Relaxed);
                    *state.last_error.lock().unwrap_or_else(|e| e.into_inner()) = None;
                }
                Err(e) => {
                    *state.last_error.lock().unwrap_or_else(|e| e.into_inner()) = Some(e);
                    next_dial = std::time::Instant::now() + wait;
                    wait = (wait * 2).min(MOST_BETWEEN_DIALS);
                }
            }
        }
        let (Some(c), Some(buffer)) = (conn.as_mut(), sample.buffer()) else { continue };
        let Ok(map) = buffer.map_readable() else { continue };
        if let Err(e) = c.write_all(map.as_slice()) {
            *state.last_error.lock().unwrap_or_else(|e| e.into_inner()) =
                Some(format!("the server closed the connection: {e}."));
            state.connected.store(false, Ordering::Relaxed);
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

/// Open the mount: the request, and the answer read up to the end of its
/// headers. A refusal comes back as a sentence that says what to change.
fn dial(s: &Settings) -> Result<TcpStream, String> {
    let address = format!("{}:{}", s.host, s.port);
    let mut stream = TcpStream::connect(&address)
        .map_err(|e| format!("could not reach the Icecast server at {address}: {e}. Check the host and port, and that the server is running."))?;
    let _ = stream.set_nodelay(true);
    stream.write_all(request(s).as_bytes()).map_err(|e| format!("the Icecast server at {address} closed the connection: {e}"))?;
    stream.set_read_timeout(Some(ANSWER_WITHIN)).map_err(|e| e.to_string())?;
    let status = read_answer(&stream).map_err(|e| format!("the Icecast server at {address} did not answer the source login: {e}"))?;
    stream.set_read_timeout(None).map_err(|e| e.to_string())?;
    match status {
        100 | 200 => Ok(stream),
        401 | 403 => Err(format!("the Icecast server at {address} refused the source login (HTTP {status}). Check the user and password; the source password is in the server's icecast.xml.")),
        400..=499 => Err(format!("the Icecast server at {address} refused the mount {} (HTTP {status}). It may be in use by another source, or not allowed by the server's settings.", s.mount)),
        other => Err(format!("the Icecast server at {address} answered HTTP {other} to the source login.")),
    }
}
