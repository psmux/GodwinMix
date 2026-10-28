//! Node ids. Each is built from what the node does, so the same work gets
//! the same id in every plan and `diff` can match nodes across plans.

use godwinmix_protocol::rendition::{AudioShape, Deinterlace, DeinterlaceMethod, Fit, Fps, SourceAspect, VideoShape};

use crate::container::{audio_slug, video_slug};
use crate::graph::Track;

pub fn track_slug(track: Track) -> &'static str {
    match track {
        Track::Video => "video",
        Track::Audio => "audio",
    }
}

/// `30` for whole rates, `30000/1001` otherwise.
pub fn fps_id(fps: Fps) -> String {
    if fps.den <= 1 {
        return fps.num.to_string();
    }
    format!("{}/{}", fps.num, fps.den)
}

/// How a scale lays the picture in, and whether it deinterlaces: what
/// makes two scales to one size different work.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Look {
    pub fit: Fit,
    /// `#rrggbb` for a pad that is not black.
    pub pad_color: Option<String>,
    pub deinterlace: Deinterlace,
    pub method: DeinterlaceMethod,
    pub aspect: SourceAspect,
}

/// `scale:cam:1280x720p30`, then `:crop`, `:stretch` or `:pad-ff0000` for a
/// fit that is not black bars, `:di` or `:da` for a deinterlace on or
/// automatic, with `-greedyh` or `-linear` for a method that is not YADIF,
/// and `:a16x9` or `:a4x3` for a shape that overrides the stream's, so a
/// plain scale keeps the id it always had.
pub fn scale_id(source: &str, s: &VideoShape, look: &Look) -> String {
    let mut id = format!("scale:{source}:{}x{}p{}", s.width, s.height, fps_id(s.fps));
    match (look.fit, look.pad_color.as_deref()) {
        (Fit::Crop, _) => id.push_str(":crop"),
        (Fit::Stretch, _) => id.push_str(":stretch"),
        (Fit::Pad, Some(c)) => id.push_str(&format!(":pad-{}", c.trim_start_matches('#').to_ascii_lowercase())),
        (Fit::Pad, None) => {}
    }
    match look.deinterlace {
        Deinterlace::On => id.push_str(":di"),
        Deinterlace::Auto => id.push_str(":da"),
        Deinterlace::Off => {}
    }
    match (look.deinterlace, look.method) {
        (Deinterlace::Off, _) | (_, DeinterlaceMethod::Yadif) => {}
        (_, DeinterlaceMethod::Greedyh) => id.push_str("-greedyh"),
        (_, DeinterlaceMethod::Linear) => id.push_str("-linear"),
    }
    match look.aspect {
        SourceAspect::Wide => id.push_str(":a16x9"),
        SourceAspect::Standard => id.push_str(":a4x3"),
        SourceAspect::Auto => {}
    }
    id
}

/// `encode:cam:h264:1280x720p30:2800k:g2000`: everything that makes two
/// encodes different, so equal work has one id.
///
/// A preset or a B frame count a request named is added, `:p4`, `:b2`.
pub fn encode_id(source: &str, s: &VideoShape, preset: Option<&str>, bframes: Option<u8>) -> String {
    let mut id = format!(
        "encode:{source}:{}:{}x{}p{}:{}k:g{}",
        video_slug(s.codec),
        s.width,
        s.height,
        fps_id(s.fps),
        s.bitrate_kbps,
        s.keyframe_ms
    );
    if let Some(p) = preset {
        id.push_str(&format!(":p{p}"));
    }
    if let Some(b) = bframes {
        id.push_str(&format!(":b{b}"));
    }
    id
}

/// The source part of a sound track's ids: `cam` for the first, `cam:a2`
/// for track 2, so `decode:cam:audio2` and `aencode:cam:a2:aac:6ch48000:128k`.
pub fn track_source(source: &str, index: u8) -> String {
    match index {
        0 => source.to_string(),
        n => format!("{source}:a{n}"),
    }
}

/// `aconvert:cam:2ch48000`.
pub fn aconvert_id(source: &str, s: &AudioShape) -> String {
    format!("aconvert:{source}:{}ch{}", s.channels, s.sample_rate)
}

/// `aencode:cam:aac:2ch48000:128k`.
pub fn aencode_id(source: &str, s: &AudioShape) -> String {
    format!(
        "aencode:{source}:{}:{}ch{}:{}k",
        audio_slug(s.codec),
        s.channels,
        s.sample_rate,
        s.bitrate_kbps
    )
}
