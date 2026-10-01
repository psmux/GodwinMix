//! A live stream as the planner sees it: codec, size, frame rate and bit
//! rate, read from what the listener reported.

use godwinmix_protocol::rendition::{AudioCodec, AudioShape, Fps, StreamInfo, VideoCodec, VideoShape};

use crate::channels::Live;

/// What a stream carries, once enough is known to plan against it. `None`
/// while the codecs or the frame rate are still arriving: a plan made on a
/// guess would be made again a second later, and every node in it moved.
pub fn info(live: &Live) -> Option<StreamInfo> {
    if live.state != "live" {
        return None;
    }
    let video = match &live.video {
        None => None,
        Some(v) => {
            let fps = live.declared_fps.or((v.fps > 0.0).then_some(v.fps))?;
            if v.width == 0 || v.height == 0 {
                return None;
            }
            Some(VideoShape {
                codec: video_codec(&v.codec),
                width: v.width,
                height: v.height,
                fps: rate(fps, live.declared_fps.is_some()),
                bitrate_kbps: v.kbps,
                keyframe_ms: 0,
            })
        }
    };
    let audio = live.audio.as_ref().map(|a| AudioShape {
        codec: audio_codec(&a.codec),
        channels: u8::try_from(a.channels).unwrap_or(2),
        sample_rate: a.sample_rate,
        bitrate_kbps: a.kbps,
    });
    if video.is_none() && audio.is_none() {
        return None;
    }
    Some(StreamInfo { video, audio, encoded: true })
}

/// The same stream for the purpose of planning: everything but the bit
/// rates, which move every second and must not move the plan with them.
pub fn same_shape(a: &StreamInfo, b: &StreamInfo) -> bool {
    let v = |i: &StreamInfo| i.video.map(|v| (v.codec, v.width, v.height, v.fps));
    let s = |i: &StreamInfo| i.audio.map(|a| (a.codec, a.channels, a.sample_rate));
    v(a) == v(b) && s(a) == s(b)
}

/// A frame rate as a fraction. A stated one is taken as stated, with the
/// NTSC rates made exact; a measured one is rounded to a whole number,
/// because a second of frames cannot tell 29.97 from 30.
pub fn rate(fps: f64, stated: bool) -> Fps {
    let whole = fps.round();
    if !stated || (fps - whole).abs() < 0.01 {
        return Fps::whole(whole.max(1.0) as u32);
    }
    let ntsc = (fps * 1.001).round();
    if (fps - ntsc * 1000.0 / 1001.0).abs() < 0.01 {
        return Fps { num: ntsc as u32 * 1000, den: 1001 };
    }
    Fps { num: (fps * 1000.0).round() as u32, den: 1000 }
}

pub fn video_codec(name: &str) -> VideoCodec {
    match name.to_ascii_lowercase().as_str() {
        "h264" | "avc" => VideoCodec::H264,
        "h265" | "hevc" => VideoCodec::H265,
        "av1" => VideoCodec::Av1,
        "vp8" => VideoCodec::Vp8,
        "vp9" => VideoCodec::Vp9,
        "mpeg2" => VideoCodec::Mpeg2,
        "prores" => VideoCodec::Prores,
        _ => VideoCodec::Other,
    }
}

pub fn audio_codec(name: &str) -> AudioCodec {
    match name.to_ascii_lowercase().as_str() {
        "aac" => AudioCodec::Aac,
        "opus" => AudioCodec::Opus,
        // Layers I and II are MPEG audio as much as layer III is, and one
        // decoder takes all three.
        "mp3" | "mp2" | "mp1" => AudioCodec::Mp3,
        "ac3" | "ac-3" => AudioCodec::Ac3,
        "pcm" => AudioCodec::Pcm,
        _ => AudioCodec::Other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_stated_ntsc_rate_is_exact_and_a_measured_one_is_whole() {
        assert_eq!(rate(29.97, true), Fps { num: 30000, den: 1001 });
        assert_eq!(rate(59.94, true), Fps { num: 60000, den: 1001 });
        assert_eq!(rate(30.0, true), Fps::whole(30));
        assert_eq!(rate(29.91, false), Fps::whole(30));
        assert_eq!(rate(12.5, true), Fps { num: 12500, den: 1000 });
    }
}
