//! The direct host: every show with compositing off, in this one process.
//!
//! A direct show is one input straight to its outputs with no compositor.
//! Its input is demuxed into the same `MediaTag`s a channel stream is made
//! of, published on a hub of its own, and every output reads that hub
//! through a bounded queue, as a channel destination does. Nothing is
//! decoded unless an output asks for a rendition (`crate::transcode`, once
//! per show) or someone asks for a picture (the frame tap).
//!
//! The seam at the top of this file is the one the inputs are built
//! against (`input/`, owned by its own author): an input hands over tags
//! and answers with its numbers, and agrees nothing else with the host.

use std::time::Duration;

use serde_json::{json, Value};

use crate::rtmp::Inlet;

// ---------------------------------------------------------------------------
// The input seam. Change it only with the author of `input/`.
// ---------------------------------------------------------------------------

/// One running input. It pushes every tag it makes into the `Inlet` it was
/// opened with, on its own thread (one at most), and never waits on the
/// host: the inlet hands a tag on and returns. Dropping the input stops it
/// and lets go of the inlet. An input that ends by itself (a file at its
/// end, a server that hung up) drops the inlet, which the host takes as the
/// input having ended, and opens it again on its backoff.
///
/// Video tags are FLV bodies as RTMP frames them (H.264 classic, HEVC as
/// enhanced RTMP), audio is AAC as FLV frames it, and each sequence header
/// comes before the first frame of its kind (`crate::tagger` does all of
/// this for a GStreamer parser's output). Timestamps are milliseconds on the
/// input's own timeline. The host rebases them, so they need not start at
/// zero, but they must not run backwards within one opening.
pub trait Input: Send {
    /// What only the input can see. The host fills what it can work out
    /// from the tags themselves (size, frame rate, codecs, bit rate, the age
    /// of the last frame and keyframe) wherever the input leaves a zero or
    /// an empty string, so an input may answer `InputStats::default()` with
    /// only its transport counters set.
    fn stats(&self) -> InputStats;
}

/// Opens an input. `Err` is a sentence for a person, naming what to change.
pub type Opener = fn(&InputSpec, Box<dyn Inlet>) -> Result<Box<dyn Input>, String>;

/// What a direct show takes, as the contract's `InputSpec`.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct InputSpec {
    /// `udp://@239.1.1.1:5000`, `srt://...`, `rtmp://...`, `rtsp://...`,
    /// `https://.../x.m3u8`, `file:///clip.ts`, `rist://...`, or
    /// `channel:<app>/<stream>`.
    pub uri: String,
    /// The MPEG-TS program to take from a multi program feed; the first
    /// when `None`.
    pub program: Option<u16>,
    /// Per transport: `interface` for multicast, `latency` for SRT,
    /// `passphrase`, and so on, as the person gave them. Null when none.
    pub params: Value,
    /// Switched to when this one stalls, and back when it returns.
    pub backup: Option<Box<InputSpec>>,
}

impl InputSpec {
    /// Read one from the table. `None` when it has no address.
    pub fn from_value(v: &Value) -> Option<InputSpec> {
        let uri = v.get("uri").and_then(Value::as_str).map(str::trim).filter(|u| !u.is_empty())?;
        Some(InputSpec {
            uri: uri.to_string(),
            program: v.get("program").and_then(Value::as_u64).and_then(|p| u16::try_from(p).ok()),
            params: v.get("params").cloned().filter(Value::is_object).unwrap_or(Value::Null),
            backup: v.get("backup").and_then(InputSpec::from_value).map(Box::new),
        })
    }
}

/// An input's numbers, in the contract's `InputStats` shape.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct InputStats {
    pub kbps: u32,
    pub fps: f64,
    pub width: u32,
    pub height: u32,
    pub video_codec: String,
    pub audio_codec: String,
    pub audio_channels: u32,
    /// MPEG-TS continuity counter errors since the input opened.
    pub cc_errors: u64,
    /// Packets the transport knows it lost (RTP sequence gaps, SRT drops).
    pub packets_lost: u64,
    /// How long ago the last keyframe arrived, in ms.
    pub keyframe_ms: u64,
    /// How long ago the last frame of any kind arrived, in ms.
    pub last_frame_ms: u64,
}

impl InputStats {
    /// The wire shape.
    pub fn to_json(&self) -> Value {
        json!({
            "kbps": self.kbps, "fps": self.fps, "width": self.width, "height": self.height,
            "video_codec": self.video_codec, "audio_codec": self.audio_codec,
            "audio_channels": self.audio_channels, "cc_errors": self.cc_errors,
            "packets_lost": self.packets_lost, "keyframe_ms": self.keyframe_ms,
            "last_frame_ms": self.last_frame_ms,
        })
    }
}

/// The longest the host waits between two attempts to open an input that
/// would not open or that ended.
pub const REOPEN_MAX: Duration = Duration::from_secs(10);

// The inputs (`input/`, the "directin" work) are declared here when they land:
// pub mod input;
