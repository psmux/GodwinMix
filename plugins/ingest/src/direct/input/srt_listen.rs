//! An SRT listener input on a port of its own, held open for the input's
//! whole life.
//!
//! `srtsrc` in listener mode ends its stream when its caller goes ("Socket is
//! broken or closed", then end of stream, measured on 1.28.6), so the input
//! built a new one on the same port. On a Windows runner the new one could
//! not bind it again ("Cannot bind to 0.0.0.0:<port>") for about 45 seconds,
//! while libsrt still held the old UDP socket for the sockets closed on it.
//! With `keep-listening` the element rebinds by itself instead, and on this
//! Windows machine that accepted a second caller and dropped it, over and
//! over.
//!
//! So the listening socket here is libsrt's own (`crate::srt::ffi`, the one
//! the channel port is built on), opened once and closed when the input is
//! removed. A caller that leaves leaves the input quiet, as a UDP sender that
//! stops does, and the next caller is read into the same pipeline, whose
//! probe and demuxer follow new PIDs as they do for UDP. The newest caller
//! wins: one that arrives while another is sending cuts the first off, which
//! is what an encoder restarted by hand looks like.
//!
//! ```text
//!   srt_accept ──► a thread: srt_recvmsg ──► appsrc ──(probe)──► parsebin ──► tags
//! ```

use std::ffi::{c_char, c_int, c_void};
use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use gstreamer as gst;
use gstreamer::prelude::*;
use gstreamer_app::AppSrc;

use crate::srt::ffi::{self, Lib, Socket};
use uri::{address, query};

mod caller;
mod uri;

/// What a caller's thread pushes into: the current connection's `appsrc`.
type Feed = Arc<Mutex<Option<AppSrc>>>;

/// At most this much TS waits in the `appsrc` before the oldest is dropped:
/// about two seconds of a 15 Mbit/s feed.
const QUEUE_BYTES: u64 = 4 << 20;

pub struct Listener {
    lib: &'static Lib,
    sock: Socket,
    feed: Feed,
    caller: Arc<Mutex<Option<Socket>>>,
    stop: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}

/// Accept every caller: the passphrase on the listening socket, which each
/// caller inherits, is what keeps anyone out.
unsafe extern "C" fn accept_all(_: *mut c_void, _: Socket, _: c_int, _: *const c_void, _: *const c_char) -> c_int {
    0
}

impl Listener {
    /// Listen on `addr`, or say why not. `None` when libsrt is not here, so
    /// the input uses `srtsrc` instead.
    pub fn open(addr: SocketAddr, passphrase: Option<&str>, latency_ms: Option<u64>) -> Option<Result<Listener, String>> {
        let lib = ffi::lib().ok()?;
        let prepare = |sock: Socket| {
            if let Some(ms) = latency_ms {
                lib.set_int(sock, ffi::LATENCY, ms.min(60_000) as i32);
            }
            match passphrase {
                Some(p) if !lib.set_text(sock, ffi::PASSPHRASE, p) => {
                    Err("the passphrase could not be used (SRT wants 10 to 79 characters). Set another.".to_string())
                }
                _ => Ok(()),
            }
        };
        let sock = match lib.listener_with(addr, accept_all, std::ptr::null_mut(), prepare) {
            Ok(sock) => sock,
            Err(e) => return Some(Err(e)),
        };
        let feed: Feed = Arc::default();
        let caller = Arc::new(Mutex::new(None));
        let stop = Arc::new(AtomicBool::new(false));
        let (f, c, s) = (feed.clone(), caller.clone(), stop.clone());
        let thread = std::thread::Builder::new().name("gmx-srt-listen".into()).spawn(move || caller::accept_loop(lib, sock, &f, &c, &s));
        Some(match thread {
            Ok(thread) => Ok(Listener { lib, sock, feed, caller, stop, thread: Some(thread) }),
            Err(e) => {
                lib.close(sock);
                Err(format!("could not start the SRT listener: {e}"))
            }
        })
    }

    /// A fresh `appsrc` for this connection of the input, in `pipeline`.
    pub fn source(&self, pipeline: &gst::Pipeline) -> Result<gst::Element, String> {
        let caps = gst::Caps::builder("video/mpegts").field("systemstream", true).field("packetsize", 188i32).build();
        let src = AppSrc::builder()
            .caps(&caps)
            .is_live(true)
            .do_timestamp(true)
            .format(gst::Format::Time)
            .max_bytes(QUEUE_BYTES)
            .build();
        // By name: the builder method is behind a feature this build may not
        // turn on, and the property is in every GStreamer since 1.20.
        src.set_property_from_str("leaky-type", "downstream");
        pipeline.add(&src).map_err(|e| e.to_string())?;
        *self.feed.lock().unwrap_or_else(|e| e.into_inner()) = Some(src.clone());
        Ok(src.upcast())
    }
}

/// The listener `uri` asks for, opened the first time and kept in `held`,
/// with a fresh `appsrc` from it in `pipeline`. `None` when libsrt is not on
/// this machine, so the input uses `srtsrc` instead.
pub fn source_for(held: &mut Option<Listener>, uri: &str, params: &serde_json::Value, pipeline: &gst::Pipeline) -> Option<Result<gst::Element, String>> {
    if held.is_none() {
        let addr = match address(uri) {
            Ok(a) => a,
            Err(e) => return Some(Err(e)),
        };
        let text = |k: &str| params.get(k).and_then(|v| v.as_str()).filter(|s| !s.is_empty()).map(str::to_string).or_else(|| query(uri, k));
        let latency = params.get("latency_ms").and_then(|v| v.as_u64()).or_else(|| query(uri, "latency").and_then(|l| l.parse().ok()));
        match Listener::open(addr, text("passphrase").as_deref(), latency)? {
            Ok(l) => *held = Some(l),
            Err(e) => return Some(Err(e)),
        }
    }
    held.as_ref().map(|l| l.source(pipeline))
}

impl Drop for Listener {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        self.lib.close(self.sock);
        if let Some(sock) = self.caller.lock().unwrap_or_else(|e| e.into_inner()).take() {
            self.lib.close(sock);
        }
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}
