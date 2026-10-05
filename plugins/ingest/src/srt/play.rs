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

use gmx_netkit::pipe::Pipe;
use gstreamer as gst;
use gstreamer::prelude::*;
use gstreamer_app::{AppSink, AppSinkCallbacks, AppSrc};

use super::decide::{by_passphrase, Decision, NOT_FOUND, UNAUTHORIZED};
use super::ffi::{Lib, Socket};
use super::streamid::Route;
use crate::channels::{Admit, Protocol, Table};
use crate::gate::ChannelGate;
use crate::hub::{Hub, Recv};
use crate::media_tag::TagKind;
use crate::rtmp::Gate;

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

fn run(lib: &'static Lib, sock: Socket, admit: &Admit, hub: &Hub, gone: Arc<AtomicBool>) -> Result<(), String> {
    gmx_netkit::init()?;
    let description = "appsrc name=in is-live=true format=bytes caps=video/x-flv ! flvdemux name=d \
         d.audio ! queue ! aacparse ! mux. mpegtsmux name=mux alignment=7 ! appsink name=out sync=false";
    let mut pipe = Pipe::launch(description)?;
    crate::restream::ts_video::video_by_codec(pipe.pipeline());
    let src: AppSrc = pipe.by_name("in").and_then(|e| e.downcast().ok()).ok_or("no appsrc")?;
    let out: AppSink = pipe.by_name("out").and_then(|e| e.downcast().ok()).ok_or("no appsink")?;
    let failed = gone.clone();
    out.set_callbacks(
        AppSinkCallbacks::builder()
            .new_sample(move |s| {
                let sample = s.pull_sample().map_err(|_| gst::FlowError::Eos)?;
                let map = sample.buffer().and_then(|b| b.map_readable().ok()).ok_or(gst::FlowError::Error)?;
                for chunk in map.chunks(1316) {
                    if lib.send(sock, chunk).is_err() {
                        failed.store(true, Ordering::Relaxed);
                        return Err(gst::FlowError::Eos);
                    }
                }
                Ok(gst::FlowSuccess::Ok)
            })
            .build(),
    );
    pipe.play(None)?;
    let _ = src.push_buffer(gst::Buffer::from_mut_slice(crate::flv::header()));
    let reader = hub.subscribe(&admit.app, &admit.stream);
    let mut zero: Option<u32> = None;
    let mut order = VideoFirst::default();
    let outcome = loop {
        if gone.load(Ordering::Relaxed) {
            break Err("the player stopped taking the stream".to_string());
        }
        if let Some(f) = pipe.failure() {
            break Err(f);
        }
        let tag = match reader.recv_timeout(Duration::from_millis(500)) {
            Recv::Tag(t) => t,
            Recv::Timeout => continue,
            Recv::Ended => break Ok(()),
        };
        let base = *zero.get_or_insert(tag.timestamp_ms);
        for tag in order.take(tag) {
            let ts = tag.timestamp_ms.saturating_sub(base);
            let bytes = match tag.kind {
                TagKind::Video => crate::flv::video(ts, &tag.payload),
                TagKind::Audio => crate::flv::audio(ts, &tag.payload),
                TagKind::Script => continue,
            };
            let _ = src.push_buffer(gst::Buffer::from_mut_slice(bytes));
        }
    };
    pipe.stop();
    outcome
}

/// Nothing goes in until both the picture and the sound have arrived, and
/// then the picture goes first.
///
/// `flvdemux` makes a pad for each kind the first time it sees one, and the
/// muxer writes its first PMT with the pads it has. With one kind in alone,
/// the first PMT named only that one and the next named both, and a
/// player's `tsdemux` takes a changed PMT as a new program: on a Windows
/// runner it offered no picture at all, and elsewhere it dropped the picture
/// it had after one frame. With both in before the muxer starts, the first
/// table is the only one. A stream that has only one kind is let through
/// after `HOLD_AT_MOST` tags.
#[derive(Default)]
struct VideoFirst {
    open: bool,
    held: Vec<crate::media_tag::MediaTag>,
}

const HOLD_AT_MOST: usize = 100;

impl VideoFirst {
    fn take(&mut self, tag: crate::media_tag::MediaTag) -> Vec<crate::media_tag::MediaTag> {
        if self.open {
            return vec![tag];
        }
        self.held.push(tag);
        let has = |kind: TagKind| self.held.iter().any(|t| t.kind == kind);
        if !(has(TagKind::Video) && has(TagKind::Audio)) && self.held.len() < HOLD_AT_MOST {
            return Vec::new();
        }
        self.open = true;
        // Stable, so each kind keeps its own order: the pictures, then the sound.
        let mut out = std::mem::take(&mut self.held);
        out.sort_by_key(|t| t.kind != TagKind::Video);
        out
    }
}
