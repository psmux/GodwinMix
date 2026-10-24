//! What the facade needs from whatever packages the segments: the tracks of
//! one output, the live window of one selection of them, and the bytes of
//! one segment. The facade names everything itself, in Flussonic's shape,
//! from these; the packager never sees a Flussonic path.
//!
//! The HLS branch's open TS access is meant to implement [`Segments`]
//! directly from its rings. Until it lands, the binary implements it over
//! the CMAF HLS a direct show already serves.

use std::future::Future;
use std::pin::Pin;

pub type Fut<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// One output of one show.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct At {
    pub show: String,
    pub output: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Video,
    Audio,
}

/// One track as a master playlist and the API describe it.
#[derive(Debug, Clone, PartialEq)]
pub struct Track {
    pub kind: Kind,
    /// From 1 within its kind: `a2` is the second sound.
    pub number: u8,
    /// RFC 6381, `avc1.64001f`, `mp4a.40.2`.
    pub codecs: String,
    /// Bits per second at the peak.
    pub bandwidth: u64,
    pub width: u32,
    pub height: u32,
    pub fps: Option<f64>,
    pub channels: Option<u32>,
    pub lang: Option<String>,
    pub name: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Tracks {
    pub tracks: Vec<Track>,
    /// Whether a selection of the video and the first sound is served as
    /// one stream (`tracks-v1a1`), which MPEG-TS can and CMAF cannot.
    pub muxed: bool,
}

/// Which tracks a playlist or a segment carries: `tracks-v1a1` is the video
/// and the first sound, `tracks-a2` the second sound alone.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Select {
    pub video: bool,
    pub audio: Option<u8>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Container {
    Ts,
    /// Fragmented MP4, with an init segment.
    Cmaf,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Segment {
    /// Unix milliseconds, UTC, when its first frame was taken. Its name is
    /// made from this, so it must never repeat for one selection.
    pub start_ms: i64,
    pub duration_ms: u32,
    /// The init segment that decodes it, for CMAF: an id that changes
    /// whenever its bytes do.
    pub init: Option<String>,
    pub discontinuity: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Window {
    pub container: Container,
    pub sequence: u64,
    pub segments: Vec<Segment>,
}

impl Window {
    /// The segment that starts in second `start_s`.
    pub fn find(&self, start_s: i64) -> Option<&Segment> {
        self.segments.iter().find(|s| s.start_ms.div_euclid(1000) == start_s)
    }
}

/// Why there is nothing to answer with. Each carries the sentence a person
/// reads in the 404 or 503.
#[derive(Debug, Clone, PartialEq)]
pub enum Missing {
    /// No such output, track or segment: 404.
    Gone(String),
    /// Not yet, or starting again: 503, which a player retries.
    NotYet(String),
    /// This build cannot serve it: 501.
    Unsupported(String),
}

pub trait Segments: Send + Sync {
    fn tracks<'a>(&'a self, at: &'a At) -> Fut<'a, Result<Tracks, Missing>>;
    fn window<'a>(&'a self, at: &'a At, sel: Select) -> Fut<'a, Result<Window, Missing>>;
    /// The segment of `sel` that starts in second `start_s`.
    fn segment<'a>(&'a self, at: &'a At, sel: Select, start_s: i64) -> Fut<'a, Result<Vec<u8>, Missing>>;
    fn init<'a>(&'a self, at: &'a At, sel: Select, id: &'a str) -> Fut<'a, Result<Vec<u8>, Missing>>;
}
