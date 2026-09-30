//! Shapes and requests the planner tests share.
#![allow(dead_code)]

pub mod random;

use godwinmix_render::*;

/// An encoded camera or stream: video of `codec`, AAC stereo 48 kHz 128k.
pub fn encoded(codec: VideoCodec, width: u32, height: u32, fps: u32, kbps: u32) -> StreamInfo {
    StreamInfo {
        video: Some(VideoShape {
            codec,
            width,
            height,
            fps: Fps::whole(fps),
            bitrate_kbps: kbps,
            keyframe_ms: 2000,
        }),
        audio: Some(AudioShape {
            codec: AudioCodec::Aac,
            channels: 2,
            sample_rate: 48_000,
            bitrate_kbps: 128,
        }),
        encoded: true,
    }
}

pub fn h264_1080p30() -> StreamInfo {
    encoded(VideoCodec::H264, 1920, 1080, 30, 6000)
}

/// Raw frames, as the programme or a camera gives them.
pub fn raw(width: u32, height: u32, fps: u32) -> StreamInfo {
    StreamInfo {
        video: Some(VideoShape {
            codec: VideoCodec::Other,
            width,
            height,
            fps: Fps::whole(fps),
            bitrate_kbps: 0,
            keyframe_ms: 0,
        }),
        audio: Some(AudioShape {
            codec: AudioCodec::Pcm,
            channels: 2,
            sample_rate: 48_000,
            bitrate_kbps: 0,
        }),
        encoded: false,
    }
}

pub fn request(id: &str, container: Container) -> RenditionRequest {
    RenditionRequest {
        id: id.into(),
        container,
        ..RenditionRequest::default()
    }
}

/// A request for `height` lines of H.264 at `kbps`, the width following.
pub fn rung(id: &str, height: u32, kbps: u32) -> RenditionRequest {
    let video = VideoWant {
        height: Some(height),
        bitrate_kbps: Some(kbps),
        ..VideoWant::default()
    };
    RenditionRequest {
        video: Some(video),
        ..request(id, Container::Flv)
    }
}

pub fn with_video(mut req: RenditionRequest, video: VideoWant) -> RenditionRequest {
    req.video = Some(video);
    req
}

pub fn sources(list: &[(&str, StreamInfo)]) -> Vec<(SourceId, StreamInfo)> {
    list.iter()
        .map(|(id, info)| (id.to_string(), info.clone()))
        .collect()
}

pub fn on(source: &str, reqs: Vec<RenditionRequest>) -> Vec<(SourceId, RenditionRequest)> {
    reqs.into_iter().map(|r| (source.to_string(), r)).collect()
}

pub fn software() -> StaticCostModel {
    StaticCostModel::software()
}

pub fn is_encode(k: &NodeKind) -> bool {
    matches!(k, NodeKind::Encode { .. })
}

pub fn is_decode(k: &NodeKind) -> bool {
    matches!(k, NodeKind::Decode { .. })
}

pub fn is_scale(k: &NodeKind) -> bool {
    matches!(k, NodeKind::Scale { .. })
}

pub fn is_copy(k: &NodeKind) -> bool {
    matches!(k, NodeKind::Copy { .. })
}

pub fn is_audio_encode(k: &NodeKind) -> bool {
    matches!(k, NodeKind::AudioEncode { .. })
}

/// The encoder an Encode node chose.
pub fn encoder_of(node: &Node) -> &EncoderSlot {
    match &node.kind {
        NodeKind::Encode { encoder, .. } => encoder,
        other => panic!("{} is not an encode: {other:?}", node.id),
    }
}

pub fn shape_of(node: &Node) -> VideoShape {
    match &node.kind {
        NodeKind::Encode { shape, .. } => *shape,
        other => panic!("{} is not an encode: {other:?}", node.id),
    }
}
