//! Node ids. Each is built from what the node does, so the same work gets
//! the same id in every plan and `diff` can match nodes across plans.

use godwinmix_protocol::rendition::{AudioShape, Fps, VideoShape};

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

/// `scale:cam:1280x720p30`.
pub fn scale_id(source: &str, s: &VideoShape) -> String {
    format!("scale:{source}:{}x{}p{}", s.width, s.height, fps_id(s.fps))
}

/// `encode:cam:h264:1280x720p30:2800k:g2000`: everything that makes two
/// encodes different, so equal work has one id.
pub fn encode_id(source: &str, s: &VideoShape) -> String {
    format!(
        "encode:{source}:{}:{}x{}p{}:{}k:g{}",
        video_slug(s.codec),
        s.width,
        s.height,
        fps_id(s.fps),
        s.bitrate_kbps,
        s.keyframe_ms
    )
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
