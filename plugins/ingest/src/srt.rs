//! SRT for every channel, on one UDP port, told apart by stream id.
//!
//! # Why libsrt and not `srtsrc`
//!
//! GStreamer's `srtsrc` in listener mode does accept several callers on one
//! port, and its `caller-connecting` signal sees each caller's stream id. But
//! it reads all of them into its one source pad, one after another, and a
//! buffer carries nothing that says which caller sent it: two encoders on one
//! port come out as one stream of interleaved MPEG-TS. Its passphrase is one
//! property for the whole element, so it cannot be a different key for each
//! channel either. That was checked against 1.28.7 with two callers
//! (`srt-live-transmit`, stream ids `a/one` and `b/two`) on one `srtsrc`.
//!
//! libsrt itself does both: `srt_listen_callback` sees each caller's stream
//! id before the handshake finishes and may set that connection's passphrase
//! or refuse it with a reason, and `srt_accept` hands back one socket per
//! caller. It is the library `srtsrc` is built on, so it is on every machine
//! that has GStreamer's SRT elements; `src/srt/ffi.rs` loads it at run time
//! and wraps the dozen calls used.
//!
//! # What happens to a caller
//!
//! ```text
//!   listen callback: stream id ──► decide ──► passphrase set, or refused with a code
//!   srt_accept ──► a thread: srt_recvmsg ──► tsdemux ──► hub (as the channel's stream)
//! ```
//!
//! From the hub on, an SRT stream is a channel stream like any other: the
//! same readers, the same events, the same auto source and destinations.

mod addr;
mod conn;
mod decide;
mod ffi;
mod play;
mod streamid;
mod ts;

use std::collections::HashMap;
use std::ffi::{c_char, c_int, c_void, CStr};
use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::channels::Admit;
use crate::gate::ChannelGate;
use decide::Decision;

/// Is libsrt on this machine? Asked by tests that need a real listener.
#[cfg(test)]
pub fn ffi_available() -> bool {
    ffi::lib().is_ok()
}

/// How long a caller let in by the callback has to finish its handshake.
const PENDING_FOR: Duration = Duration::from_secs(30);

/// What the listen callback shares with the accept loop.
struct Ctx {
    lib: &'static ffi::Lib,
    gate: Arc<ChannelGate>,
    /// Callers let in by the hook, until accepted: who, whether to play to
    /// them rather than take from them, and when.
    pending: Mutex<HashMap<ffi::Socket, (Admit, bool, Instant)>>,
}

/// A listening SRT port. Dropping it closes the port; callers already on air
/// are cut off by the gate when their channel stops taking SRT.
pub struct SrtServer {
    port: u16,
    listener: ffi::Socket,
    stop: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl SrtServer {
    pub fn bind(bind: &str, port: u16, gate: Arc<ChannelGate>) -> Result<SrtServer, String> {
        let lib = ffi::lib()?;
        let addr: SocketAddr = format!("{bind}:{port}")
            .parse()
            .map_err(|e| format!("'{bind}:{port}' is not an address to listen for SRT on: {e}"))?;
        // Leaked on purpose: libsrt may call the hook from its own thread up
        // to the moment the listener is closed, so it must outlive any close.
        // It is three pointers and a map, once per time the port opens.
        let ctx: &'static Ctx = Box::leak(Box::new(Ctx { lib, gate, pending: Mutex::default() }));
        let opaque = ctx as *const Ctx as *mut c_void;
        let listener = lib.listener(addr, hook, opaque)?;
        let stop = Arc::new(AtomicBool::new(false));
        let halt = stop.clone();
        let thread = std::thread::Builder::new()
            .name("gmx-srt-accept".into())
            .spawn(move || accept_loop(ctx, listener, &halt))
            .map_err(|e| format!("could not start the SRT listener: {e}"))?;
        Ok(SrtServer { port, listener, stop, thread: Some(thread) })
    }

    pub fn port(&self) -> u16 {
        self.port
    }
}

impl Drop for SrtServer {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Ok(lib) = ffi::lib() {
            lib.close(self.listener);
        }
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn accept_loop(ctx: &'static Ctx, listener: ffi::Socket, stop: &AtomicBool) {
    while let Some((sock, peer)) = ctx.lib.accept(listener) {
        if stop.load(Ordering::Relaxed) {
            ctx.lib.close(sock);
            return;
        }
        let admitted = ctx.pending.lock().unwrap_or_else(|e| e.into_inner()).remove(&sock);
        let Some((admit, player, _)) = admitted else {
            ctx.lib.close(sock);
            continue;
        };
        let gate = ctx.gate.clone();
        let started = std::thread::Builder::new()
            .name("gmx-srt-conn".into())
            .spawn(move || if player { play::serve(ctx.lib, sock, peer, admit, gate) } else { conn::serve(ctx.lib, sock, peer, admit, gate) });
        if started.is_err() {
            ctx.lib.close(sock);
        }
    }
}

/// libsrt's listen callback: decide on a caller from its stream id.
unsafe extern "C" fn hook(opaque: *mut c_void, ns: ffi::Socket, _hs: c_int, peer: *const c_void, id: *const c_char) -> c_int {
    // SAFETY: `opaque` is the leaked `Ctx` handed over in `bind`.
    let ctx = unsafe { &*(opaque as *const Ctx) };
    // SAFETY: libsrt passes a nul terminated stream id, or null.
    let id = if id.is_null() { String::new() } else { unsafe { CStr::from_ptr(id) }.to_string_lossy().into_owned() };
    // SAFETY: libsrt's peer address is a sockaddr_any, at least 28 bytes.
    let peer = if peer.is_null() { "unknown".into() } else { addr::decode(unsafe { std::slice::from_raw_parts(peer.cast::<u8>(), 28) }) };
    let answer = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| on_caller(ctx, ns, &peer, &id)));
    answer.unwrap_or(-1)
}

fn on_caller(ctx: &Ctx, ns: ffi::Socket, peer: &str, id: &str) -> c_int {
    let decision = match streamid::parse(id) {
        None => Decision::Refuse {
            code: decide::BAD_REQUEST,
            channel: String::new(),
            stream: String::new(),
            why: "the stream id names no channel. Set it to <channel>/<stream>, such as sunday-service/main.".into(),
        },
        Some(route) => {
            let table = ctx.gate.table.read().unwrap_or_else(|e| e.into_inner());
            decide::decide(&table, &ctx.gate.hub, &route)
        }
    };
    match decision {
        Decision::Refuse { code, channel, stream, why } => {
            ctx.gate.turn_away(&channel, &stream, peer, why);
            ctx.lib.reject(ns, code);
            -1
        }
        Decision::Take { admit, passphrase } => let_in(ctx, ns, peer, admit, passphrase, false),
        Decision::Play { admit, passphrase } => let_in(ctx, ns, peer, admit, passphrase, true),
    }
}

/// Set the passphrase the key is, and remember the caller for the accept.
fn let_in(ctx: &Ctx, ns: ffi::Socket, peer: &str, admit: Admit, passphrase: Option<String>, player: bool) -> c_int {
    if let Some(secret) = passphrase {
        if !ctx.lib.set_text(ns, ffi::PASSPHRASE, &secret) {
            let why = "the key could not be used as an SRT passphrase (it must be 10 to 79 characters). Make a new key.".to_string();
            ctx.gate.turn_away(&admit.channel, &admit.stream, peer, why);
            return -1;
        }
    }
    let mut pending = ctx.pending.lock().unwrap_or_else(|e| e.into_inner());
    pending.retain(|_, (_, _, at)| at.elapsed() < PENDING_FOR);
    pending.insert(ns, (admit, player, Instant::now()));
    0
}

#[cfg(test)]
#[path = "srt/tests.rs"]
mod tests;
