//! An encoder's output as the tags a destination sends.
//!
//! The same conversion SRT and WHIP use (`crate::tagger`): the codec
//! configuration in the caps becomes a sequence header when it is new, and
//! each access unit becomes an FLV body with its composition offset. The
//! timeline is the publisher's own: the stream was fed in from its first
//! tag's time, and that time is added back here, so an encoded video track
//! and the stream's own copied sound stay in step.

use std::sync::{Arc, Mutex};

use gstreamer as gst;
use gstreamer_app as gst_app;

use crate::media_tag::{MediaTag, TagKind};
use crate::tagger::prefix;

/// Where a node's tags go, and the offset of this session's timeline.
pub trait Route: Send + Sync {
    fn tag(&self, node: &str, tag: MediaTag);
    /// The publisher's time, in ms, that the pipeline's zero stands for.
    fn base_ms(&self) -> u32;
}

/// Hand every sample of `sink` on to `route` as the tags of `node`.
pub fn attach(sink: &gst_app::AppSink, kind: TagKind, node: String, route: Arc<dyn Route>) {
    let header: Mutex<Option<Vec<u8>>> = Mutex::new(None);
    sink.set_callbacks(
        gst_app::AppSinkCallbacks::builder()
            .new_sample(move |sink| {
                let sample = sink.pull_sample().map_err(|_| gst::FlowError::Eos)?;
                for tag in tags(&sample, kind, &header, route.base_ms()) {
                    route.tag(&node, tag);
                }
                Ok(gst::FlowSuccess::Ok)
            })
            .build(),
    );
}

fn tags(sample: &gst::Sample, kind: TagKind, header: &Mutex<Option<Vec<u8>>>, base: u32) -> Vec<MediaTag> {
    let Some(buffer) = sample.buffer() else { return Vec::new() };
    // Running time, not the buffer's own stamp: an encoder moves its segment
    // (and its stamps) a thousand hours on so that no DTS is negative.
    let Some(at) = running_time(sample, buffer) else { return Vec::new() };
    let ms = base.wrapping_add((at.mseconds() & 0xffff_ffff) as u32);
    let mut out = Vec::with_capacity(2);
    if let Some(config) = codec_data(sample) {
        let mut last = header.lock().unwrap_or_else(|e| e.into_inner());
        if last.as_deref() != Some(config.as_slice()) {
            out.push(make(kind, ms, true, true, &prefix(kind, true, true, 0), &config));
            *last = Some(config);
        }
    }
    let Ok(map) = buffer.map_readable() else { return out };
    let keyframe = kind == TagKind::Video && !buffer.flags().contains(gst::BufferFlags::DELTA_UNIT);
    let cts = match (buffer.pts(), buffer.dts()) {
        (Some(pts), Some(dts)) => pts.mseconds() as i64 - dts.mseconds() as i64,
        _ => 0,
    };
    out.push(make(kind, ms, keyframe, false, &prefix(kind, keyframe, false, cts), map.as_slice()));
    out
}

fn make(kind: TagKind, timestamp_ms: u32, keyframe: bool, sequence_header: bool, head: &[u8], body: &[u8]) -> MediaTag {
    let mut payload = Vec::with_capacity(head.len() + body.len());
    payload.extend_from_slice(head);
    payload.extend_from_slice(body);
    MediaTag { kind, timestamp_ms, keyframe, sequence_header, payload: Arc::from(payload) }
}

fn running_time(sample: &gst::Sample, buffer: &gst::BufferRef) -> Option<gst::ClockTime> {
    let at = buffer.dts().or(buffer.pts())?;
    let segment = sample.segment()?.downcast_ref::<gst::format::Time>()?;
    segment.to_running_time(at)
}

fn codec_data(sample: &gst::Sample) -> Option<Vec<u8>> {
    let caps = sample.caps()?;
    let data = caps.structure(0)?.get::<gst::Buffer>("codec_data").ok()?;
    let map = data.map_readable().ok()?;
    Some(map.as_slice().to_vec())
}
