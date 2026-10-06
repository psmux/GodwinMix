//! SRT players on the channel port: a caller with `m=request` in its stream
//! id is sent the channel stream it names, as MPEG-TS, on the same UDP port
//! encoders publish to. This is how another site, a decoder or vMix pulls a
//! channel feed without a second port being opened.
//!
//! ```text
//!   hub reader ──► appsrc(FLV) ──► flvdemux ─┬─► h264parse or h265parse ─┐
//!                                            └─► aacparse ───────────────┴─► mpegtsmux ──► appsink ──► srt_send
//! ```
//!
//! Nothing is decoded. The key is checked exactly as for a publisher: as the
//! SRT passphrase, or `?psk=` in the stream id. A player that stops reading
//! is dropped by libsrt; the publisher and every other reader never wait.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use gstreamer as gst;

use super::decide::{by_passphrase, Decision, NOT_FOUND, UNAUTHORIZED};
use super::ffi::{Lib, Socket};
use super::streamid::Route;
use crate::channels::{Admit, Protocol, Table};
use crate::gate::ChannelGate;
use crate::hub::{Hub, Recv};
use crate::media_tag::{MediaTag, TagKind};
use crate::rtmp::Gate;
use first::VideoFirst;
use pipeline::launch;

mod first;
mod gate;
mod pipeline;

/// Let a player in when its key is good and the stream is on air.
pub fn decide(table: &Table, hub: &Hub, route: &Route) -> Decision {
    let refuse = |code, why: String| Decision::Refuse { code, channel: route.app.clone(), stream: route.name().to_string(), why };
    let channel = table.channels.iter().find(|c| c.app == route.app);
    let decided = if route.has_key() || channel.is_some_and(|c| c.key_in_name) || channel.is_none() {
        let code = if channel.is_none() { NOT_FOUND } else { UNAUTHORIZED };
        table.admit_via(Protocol::Srt, &route.app, &route.stream).map(|a| (a, None)).map_err(|r| (code, r.why))
    } else {
        by_passphrase(table, route)
    };
    match decided {
        Err((code, why)) => refuse(code, why),
        Ok((admit, _)) if !hub.is_live(&admit.app, &admit.stream) => refuse(
            NOT_FOUND,
            format!("{}/{} is not on air. Play it once its encoder is publishing.", admit.app, admit.stream),
        ),
        Ok((admit, passphrase)) => Decision::Play { admit, passphrase },
    }
}

/// Send `admit`'s stream to the player on `sock` until either end goes.
pub fn serve(lib: &'static Lib, sock: Socket, peer: String, admit: Admit, gate: Arc<ChannelGate>) {
    let name = format!("{}/{}", admit.app, admit.stream);
    let gone = Arc::new(AtomicBool::new(false));
    let why = match run(lib, sock, &admit, &gate.hub, gone.clone()) {
        Ok(()) => "the stream ended".to_string(),
        Err(e) => e,
    };
    gone.store(true, Ordering::Relaxed);
    lib.close(sock);
    gate.note(format!("the SRT player {peer} of {name} left: {why}"));
}

/// How long the muxer waits for every stream's caps before it takes what it
/// has. A parser that has not spoken by then is not going to.
const CAPS_WAIT: Duration = Duration::from_secs(3);

fn run(lib: &'static Lib, sock: Socket, admit: &Admit, hub: &Hub, gone: Arc<AtomicBool>) -> Result<(), String> {
    gmx_netkit::init()?;
    let reader = hub.subscribe(&admit.app, &admit.stream);
    let mut order = VideoFirst::default();
    let mut next = || -> Result<Option<Vec<MediaTag>>, String> {
        if gone.load(Ordering::Relaxed) {
            return Err("the player stopped taking the stream".to_string());
        }
        match reader.recv_timeout(Duration::from_millis(500)) {
            Recv::Tag(t) => Ok(Some(order.take(t))),
            Recv::Timeout => Ok(Some(Vec::new())),
            Recv::Ended => Ok(None),
        }
    };
    // Built once the first tags say which kinds the stream has.
    let first = loop {
        match next()? {
            Some(tags) if tags.is_empty() => continue,
            Some(tags) => break tags,
            None => return Ok(()),
        }
    };
    let has = |kind: TagKind| first.iter().any(|t| t.kind == kind);
    let kinds = usize::from(has(TagKind::Video)) + usize::from(has(TagKind::Audio));
    let (mut pipe, src, caps) = launch(lib, sock, has(TagKind::Audio), kinds, gone.clone())?;
    // From the earliest, so a sound frame held behind the pictures keeps its
    // place rather than all of them landing on zero.
    let base = first.iter().map(|t| t.timestamp_ms).min().unwrap_or(0);
    let push = |tags: Vec<MediaTag>| {
        for tag in tags {
            let ts = tag.timestamp_ms.saturating_sub(base);
            let bytes = match tag.kind {
                TagKind::Video => crate::flv::video(ts, &tag.payload),
                TagKind::Audio => crate::flv::audio(ts, &tag.payload),
                TagKind::Script => continue,
            };
            let _ = src.push_buffer(gst::Buffer::from_mut_slice(bytes));
        }
    };
    push(first);
    let started = std::time::Instant::now();
    let outcome = loop {
        if let Some(f) = pipe.failure() {
            break Err(f);
        }
        if started.elapsed() > CAPS_WAIT && !caps.is_open() {
            caps.open();
        }
        match next() {
            Ok(Some(tags)) => push(tags),
            Ok(None) => break Ok(()),
            Err(e) => break Err(e),
        }
    };
    caps.open();
    pipe.stop();
    outcome
}

