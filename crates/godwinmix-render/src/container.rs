//! Which codecs each container can carry. The first entry in each list is
//! what an output gets when it names no codec and the source's own codec
//! cannot travel in that container.

use godwinmix_protocol::rendition::{AudioCodec, Container, Fps, VideoCodec, VideoShape};

use AudioCodec as A;
use VideoCodec as V;

/// The video codecs `container` can carry, the default first.
///
/// FLV assumes enhanced RTMP, which carries HEVC and AV1 as well as H.264.
/// An output going to a server that only takes classic RTMP names `h264`.
pub fn video_codecs(container: Container) -> &'static [VideoCodec] {
    match container {
        Container::Flv => &[V::H264, V::H265, V::Av1],
        Container::Webrtc => &[V::H264, V::Vp8, V::Vp9, V::Av1],
        Container::MpegTs => &[V::H264, V::H265, V::Av1, V::Mpeg2],
        Container::Mp4Fragmented => &[V::H264, V::H265, V::Av1, V::Vp9],
        Container::Mkv => &[
            V::H264,
            V::H265,
            V::Av1,
            V::Vp8,
            V::Vp9,
            V::Mpeg2,
            V::Prores,
        ],
        Container::Hls | Container::LlHls => &[V::H264, V::H265, V::Av1],
        Container::Dash => &[V::H264, V::H265, V::Av1, V::Vp9],
        Container::Rtp => &[V::H264, V::H265, V::Av1, V::Vp8, V::Vp9, V::Mpeg2],
    }
}

/// The audio codecs `container` can carry, the default first.
pub fn audio_codecs(container: Container) -> &'static [AudioCodec] {
    match container {
        Container::Flv => &[A::Aac, A::Mp3],
        Container::Webrtc => &[A::Opus],
        Container::MpegTs => &[A::Aac, A::Mp3, A::Ac3, A::Opus],
        Container::Mp4Fragmented => &[A::Aac, A::Opus, A::Mp3, A::Ac3],
        Container::Mkv => &[A::Aac, A::Opus, A::Mp3, A::Ac3, A::Pcm],
        Container::Hls | Container::LlHls => &[A::Aac, A::Mp3, A::Ac3],
        Container::Dash => &[A::Aac, A::Opus, A::Ac3],
        Container::Rtp => &[A::Opus, A::Aac, A::Mp3, A::Ac3, A::Pcm],
    }
}

pub fn carries_video(container: Container, codec: VideoCodec) -> bool {
    video_codecs(container).contains(&codec)
}

pub fn carries_audio(container: Container, codec: AudioCodec) -> bool {
    audio_codecs(container).contains(&codec)
}

/// The name a person reads: `H.264`, `VP9`.
pub fn video_name(codec: VideoCodec) -> &'static str {
    match codec {
        V::H264 => "H.264",
        V::H265 => "H.265",
        V::Av1 => "AV1",
        V::Vp8 => "VP8",
        V::Vp9 => "VP9",
        V::Mpeg2 => "MPEG-2",
        V::Prores => "ProRes",
        V::Other => "an unnamed codec",
    }
}

pub fn audio_name(codec: AudioCodec) -> &'static str {
    match codec {
        A::Aac => "AAC",
        A::Opus => "Opus",
        A::Mp3 => "MP3",
        A::Ac3 => "AC-3",
        A::Pcm => "PCM",
        A::Other => "an unnamed codec",
    }
}

/// "1920x1080 at 30 fps", "1920x1080 at 29.97 fps".
pub fn shape_text(shape: &VideoShape) -> String {
    format!(
        "{}x{} at {} fps",
        shape.width,
        shape.height,
        fps_text(shape.fps)
    )
}

pub fn fps_text(fps: Fps) -> String {
    if fps.den <= 1 {
        return fps.num.to_string();
    }
    let v = fps.as_f64();
    format!("{}", (v * 100.0).round() / 100.0)
}

/// The slug the wire uses: `h264`, `mpeg-ts`. Written out rather than read
/// through serde because node ids are built from them on every plan.
pub fn video_slug(codec: VideoCodec) -> &'static str {
    match codec {
        V::H264 => "h264",
        V::H265 => "h265",
        V::Av1 => "av1",
        V::Vp8 => "vp8",
        V::Vp9 => "vp9",
        V::Mpeg2 => "mpeg2",
        V::Prores => "prores",
        V::Other => "other",
    }
}

pub fn audio_slug(codec: AudioCodec) -> &'static str {
    match codec {
        A::Aac => "aac",
        A::Opus => "opus",
        A::Mp3 => "mp3",
        A::Ac3 => "ac3",
        A::Pcm => "pcm",
        A::Other => "other",
    }
}

pub fn container_slug(container: Container) -> &'static str {
    match container {
        Container::Flv => "flv",
        Container::MpegTs => "mpeg-ts",
        Container::Mp4Fragmented => "mp4-fragmented",
        Container::Mkv => "mkv",
        Container::Hls => "hls",
        Container::LlHls => "ll-hls",
        Container::Dash => "dash",
        Container::Rtp => "rtp",
        Container::Webrtc => "webrtc",
    }
}
