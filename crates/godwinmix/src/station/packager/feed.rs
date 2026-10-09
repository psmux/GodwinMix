//! One connection to the relay: learn the headers, start the pipeline at
//! the first keyframe, and push every frame after it.

use super::board::Board;
use super::caps::{self, Read};
use super::flv::{self, Kind, Tag};
use super::output::End;
use super::session::Session;
use godwinmix_core::hls::Stream;
use godwinmix_protocol::destination::DestinationState as S;
use gstreamer as gst;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// What one connection has learned.
#[derive(Default)]
struct Learned {
    video: Option<gst::Caps>,
    audio: Option<gst::Caps>,
    session: Option<Session>,
}

pub(super) fn session(mut reader: flv::Reader, stream: &Arc<Stream>, board: &Board, stop: &AtomicBool, sound: &dyn Fn(bool, &str) -> String) -> End {
    let mut at = Learned::default();
    loop {
        if stop.load(Ordering::Relaxed) {
            return End::Stopped;
        }
        if let Some(why) = at.session.as_ref().and_then(Session::failure) {
            return End::Lost(format!("the packager stopped: {why}"));
        }
        let tag = match reader.next() {
            Ok(t) => t,
            Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => return End::Lost("the input's stream ended; waiting for it again".into()),
            Err(e) => return End::Lost(format!("nothing came from the input for five seconds ({e})")),
        };
        let read = match tag.kind {
            Kind::Video => caps::video(&tag.body),
            Kind::Audio => caps::audio(&tag.body),
            Kind::Script => continue,
        };
        if let Some(end) = take(&mut at, stream, &tag, &read, board, sound) {
            return end;
        }
    }
}

/// One tag: learn a header, start the pipeline at the first keyframe, push
/// a frame. Answers how the connection ends, when this tag ends it.
fn take(at: &mut Learned, stream: &Arc<Stream>, tag: &Tag, read: &Read, board: &Board, sound: &dyn Fn(bool, &str) -> String) -> Option<End> {
    match (read, tag.kind) {
        (Read::Unsupported(codec), kind) => return Some(End::Refused(sound(kind == Kind::Audio, codec))),
        (Read::Header(c), kind) => {
            let slot = if kind == Kind::Video { &mut at.video } else { &mut at.audio };
            let moved = slot.as_ref().is_some_and(|was| was != c) || (kind == Kind::Audio && at.session.as_ref().is_some_and(|s| !s.has_audio()));
            *slot = Some(c.clone());
            return (moved && at.session.is_some()).then_some(End::Again);
        }
        (Read::Frame { key, .. }, Kind::Video) if at.session.is_none() => {
            let video = at.video.as_ref()?;
            if !key {
                return None;
            }
            match Session::start(stream, video, at.audio.as_ref(), tag.ms) {
                Ok(s) => at.session = Some(s),
                Err(e) => return Some(End::Lost(e)),
            }
        }
        _ => {}
    }
    let s = at.session.as_ref()?;
    board.packaged(s.push(tag, read));
    if board.state() != S::Live && stream.ready() {
        board.set(S::Live, None);
    }
    None
}
