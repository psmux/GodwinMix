//! The `onMetaData` a converted pair starts with.

use rml_rtmp::sessions::StreamMetadata;
use serde_json::Value;

use super::router::{Inner, Output};
use crate::flv;
use crate::media_tag::{MediaTag, TagKind};

/// `onMetaData` for a pair: the publisher's own, with the size, rate and
/// bit rates of whatever this pair encodes put in their place.
pub(super) fn metadata(inner: &Inner, output: &Output, at: u32) -> MediaTag {
    let mut meta = inner
        .script
        .as_ref()
        .and_then(|s| crate::restream::meta::parse(&s.payload))
        .unwrap_or_else(StreamMetadata::new);
    if let Some(v) = output.video.as_ref().and_then(|id| inner.nodes.get(id)) {
        let n = |k: &str| v.get(k).and_then(Value::as_u64).map(|x| x as u32);
        meta.video_width = n("width");
        meta.video_height = n("height");
        meta.video_bitrate_kbps = n("bitrate_kbps");
        meta.video_codec_id = Some(7);
        let fps = v.get("fps").and_then(Value::as_array).and_then(|p| Some(p.first()?.as_f64()? / p.get(1)?.as_f64()?.max(1.0)));
        meta.video_frame_rate = fps.map(|f| f as f32);
    }
    if let Some(a) = output.audio.as_ref().and_then(|id| inner.nodes.get(id)) {
        let n = |k: &str| a.get(k).and_then(Value::as_u64).map(|x| x as u32);
        meta.audio_bitrate_kbps = n("bitrate_kbps");
        meta.audio_sample_rate = n("sample_rate");
        meta.audio_channels = n("channels");
        meta.audio_codec_id = Some(10);
    }
    if output.audio.is_none() {
        (meta.audio_codec_id, meta.audio_bitrate_kbps, meta.audio_channels, meta.audio_sample_rate) = (None, None, None, None);
    }
    let payload = flv::metadata_body(&meta);
    MediaTag { track: 0, kind: TagKind::Script, timestamp_ms: at, keyframe: false, sequence_header: false, payload: payload.into() }
}
