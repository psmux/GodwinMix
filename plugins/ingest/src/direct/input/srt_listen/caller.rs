//! The callers: each accepted one is read on a thread of its own into
//! whichever `appsrc` the input has now, and the newest caller cuts the one
//! before it off.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use gstreamer as gst;

use super::Feed;
use crate::srt::ffi::{self, Lib, Socket};

pub fn accept_loop(lib: &'static Lib, listener: Socket, into: &Feed, caller: &Arc<Mutex<Option<Socket>>>, stop: &AtomicBool) {
    while let Some((sock, _peer)) = lib.accept(listener) {
        if stop.load(Ordering::Relaxed) {
            lib.close(sock);
            return;
        }
        // A second without packets returns, so a caller cut off is noticed.
        lib.set_int(sock, ffi::RCVTIMEO, 1000);
        if let Some(old) = caller.lock().unwrap_or_else(|e| e.into_inner()).replace(sock) {
            lib.close(old);
        }
        let (into, caller) = (into.clone(), caller.clone());
        let started = std::thread::Builder::new().name("gmx-srt-caller".into()).spawn(move || read(lib, sock, &into, &caller));
        if started.is_err() {
            lib.close(sock);
        }
    }
}

/// Read one caller until it goes or is replaced. Never waits on the
/// pipeline: the `appsrc` drops its oldest data when it is full.
fn read(lib: &'static Lib, sock: Socket, into: &Feed, caller: &Mutex<Option<Socket>>) {
    let mut buffer = [0u8; 1500];
    loop {
        match lib.recv(sock, &mut buffer) {
            Ok(0) if *caller.lock().unwrap_or_else(|e| e.into_inner()) != Some(sock) => break,
            Ok(0) => continue,
            Ok(n) => {
                let src = into.lock().unwrap_or_else(|e| e.into_inner()).clone();
                if let Some(src) = src {
                    let _ = src.push_buffer(gst::Buffer::from_slice(buffer[..n].to_vec()));
                }
            }
            Err(_) => break,
        }
    }
    let mut current = caller.lock().unwrap_or_else(|e| e.into_inner());
    if *current == Some(sock) {
        *current = None;
        lib.close(sock);
    }
}
