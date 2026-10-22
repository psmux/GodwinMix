//! One connection to the relay: learn the headers, start the pipeline at
//! the first keyframe, and push every frame after it.

use super::board::Board;
use super::caps::{self, Read};
use super::flv::{self, Kind, Tag};
use super::output::End;
use super::session::Session;
use super::tracks::Sound;
use godwinmix_core::hls::Stream;
use godwinmix_protocol::destination::DestinationState as S;
use gstreamer as gst;
use std::sync::atomic::{AtomicBool, Ordering};
use std::collections::BTreeMap;
use std::time::Duration;
use std::sync::Arc;

/// The longest a live stream may go between two tags.
const LIVE_WAIT: Duration = Duration::from_secs(5);
/// The longest a new connection may wait for its first frame.
const FIRST_WAIT: Duration = Duration::from_secs(30);

/// What one connection has learned.
#[derive(Default)]
struct Learned {
    video: Option<gst::Caps>,
    /// Each sound track's caps, by track.
    audio: BTreeMap<u8, gst::Caps>,
    session: Option<Session>,
}

pub(super) fn session(mut reader: flv::Reader, stream: &Arc<Stream>, board: &Board, stop: &AtomicBool, named: &[Sound], sound: &dyn Fn(bool, &str) -> String) -> End {
    let mut at = Learned::default();
    loop {
        if stop.load(Ordering::Relaxed) {
            return End::Stopped;
        }
        if let Some(why) = at.session.as_ref().and_then(Session::failure) {
            return End::Lost(format!("the packager stopped: {why}"));
        }
        // Before the first picture the relay may be waiting on a keyframe,
        // or a rendition on its encoder, which on a station starting fifty
        // outputs at once took longer than the five seconds a live stream
        // gets between tags.
        let patience = if at.session.is_some() { LIVE_WAIT } else { FIRST_WAIT };
        let tag = match reader.ready(patience, stop).and_then(|go| go.then(|| reader.next()).transpose()) {
            Ok(Some(t)) => t,
            Ok(None) => return End::Stopped,
            Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => return End::Lost("the input's stream ended; waiting for it again".into()),
            Err(e) => return End::Lost(format!("nothing came from the input for {} s ({e})", patience.as_secs())),
        };
        let read = match tag.kind {
            Kind::Video => caps::video(&tag.body),
            Kind::Audio => caps::audio(&tag.body),
            Kind::Script => continue,
        };
        if let Some(end) = take(&mut at, stream, &tag, &read, board, named, sound) {
            return end;
        }
    }
}

/// One tag: learn a header, start the pipeline at the first keyframe, push
/// a frame. Answers how the connection ends, when this tag ends it. A
/// sound track whose header comes after the start, or caps that change,
/// build the pipeline again.
fn take(at: &mut Learned, stream: &Arc<Stream>, tag: &Tag, read: &Read, board: &Board, named: &[Sound], sound: &dyn Fn(bool, &str) -> String) -> Option<End> {
    match (read, tag.kind) {
        (Read::Unsupported(codec), kind) => return Some(End::Refused(sound(kind == Kind::Audio, codec))),
        (Read::Header(c), Kind::Video) => {
            let moved = at.video.as_ref().is_some_and(|was| was != c);
            at.video = Some(c.clone());
            if moved && at.session.is_some() {
                return Some(End::Again);
            }
        }
        (Read::Header(c), Kind::Audio) => {
            let moved = at.audio.get(&tag.track).is_some_and(|was| was != c) || at.session.as_ref().is_some_and(|s| !s.has_sound(tag.track));
            at.audio.insert(tag.track, c.clone());
            if moved && at.session.is_some() {
                return Some(End::Again);
            }
        }
        (Read::Frame { key, .. }, Kind::Video) if at.session.is_none() => {
            let video = at.video.as_ref()?;
            if !key {
                return None;
            }
            let sounds: Vec<(u8, gst::Caps)> = at.audio.iter().map(|(t, c)| (*t, c.clone())).collect();
            match Session::start(stream, video, &sounds, tag.ms, named) {
                Ok(s) => at.session = Some(s),
                Err(e) => return Some(End::Lost(e)),
            }
        }
        _ => {}
    }
    let s = at.session.as_mut()?;
    board.packaged(s.push(tag, read));
    if board.state() != S::Live && stream.ready() {
        board.set(S::Live, None);
    }
    None
}
