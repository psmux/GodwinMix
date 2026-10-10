//! One SRT caller, from the moment it is accepted to the last packet.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;

use crate::channels::{Admit, Protocol};
use crate::gate::ChannelGate;
use crate::rtmp::{Gate, Kick, IDLE};

use super::ffi::{Lib, Socket, RCVTIMEO};
use super::ts::Demux;

/// A live mode SRT payload is at most 1456 bytes; 1316 is what every
/// MPEG-TS sender uses.
const PACKET: usize = 1500;

pub fn serve(lib: &'static Lib, sock: Socket, peer: String, admit: Admit, gate: Arc<ChannelGate>) {
    // A second without packets returns, so a kick is noticed within one.
    lib.set_int(sock, RCVTIMEO, 1000);
    let closed = Arc::new(AtomicBool::new(false));
    let shut = closed.clone();
    let kick: Kick = Arc::new(move || {
        if !shut.swap(true, Ordering::Relaxed) {
            lib.close(sock);
        }
    });
    let name = format!("{}-{}", admit.app, admit.stream);
    let inlet = match gate.let_in(Protocol::Srt, admit, &peer, kick.clone()) {
        Ok(inlet) => inlet,
        Err(_) => return kick(),
    };
    let demux = match Demux::start(inlet, gate.reporter.clone(), &name) {
        Ok(demux) => demux,
        Err(why) => {
            gate.note(format!("the SRT stream {name} from {peer} could not be read: {why}"));
            return kick();
        }
    };
    let mut buffer = [0u8; PACKET];
    let mut last = Instant::now();
    let why = loop {
        match lib.recv(sock, &mut buffer) {
            Ok(0) if closed.load(Ordering::Relaxed) => break "cut off".to_string(),
            // libsrt breaks a connection whose peer is silent for its own
            // idle timeout, five seconds by default; this is the same
            // promise kept here, whatever the caller set that option to, so
            // a pulled cable frees the name for the encoder's reconnect.
            Ok(0) if last.elapsed() >= IDLE => break format!("nothing came for {} s, so the network to the caller has gone", IDLE.as_secs()),
            Ok(0) => continue,
            Ok(n) => {
                last = Instant::now();
                demux.push(&buffer[..n]);
            }
            Err(e) => break e,
        }
        if let Some(failure) = demux.failure() {
            break failure;
        }
    };
    drop(demux);
    kick();
    if let Some(r) = &gate.reporter {
        r.info(format!("the SRT stream {name} from {peer} ended: {why}"));
    }
}
