//! Encoded frames out of GStreamer, as the tags the hub carries.
//!
//! The hub speaks FLV tag bodies, because RTMP does and RTMP came first. A
//! stream that arrives some other way (SRT's MPEG-TS, WHIP's RTP) is parsed by
//! GStreamer into whole H.264 access units and raw AAC frames, and this turns
//! each one into the body RTMP would have carried: a codec byte, a packet
//! type, a composition offset for video, and the bytes as they came. Nothing
//! is decoded. The codec headers come from the caps (`codec_data`), which is
//! where `h264parse` and `aacparse` put the AVC and AAC configuration.
//!
//! The appsinks never block the streaming thread on anything but the hub's
//! slot lock, which a publisher holds only to hand a tag on.

use std::sync::{Arc, Mutex};

use gstreamer as gst;
use gstreamer::prelude::*;
use gstreamer_app as gst_app;

use crate::media_tag::{MediaTag, TagKind};
use crate::rtmp::Inlet;

/// Where the tags go: one inlet, shared by the audio and the video sink.
/// Taken out (`None`) when the publisher leaves, which is what tells the
/// hub and the core, so that happens when the owner says and not whenever
/// GStreamer lets go of the last sink.
pub type Shared = Arc<Mutex<Option<Box<dyn Inlet>>>>;

/// A `Shared` holding `inlet`.
pub fn share(inlet: Box<dyn Inlet>) -> Shared {
    Arc::new(Mutex::new(Some(inlet)))
}

/// Take the inlet out, ending the publication.
pub fn close(shared: &Shared) {
    let gone = shared.lock().unwrap_or_else(|e| e.into_inner()).take();
    drop(gone);
}

/// The caps each sink asks its parser for.
pub const VIDEO_CAPS: &str = "video/x-h264,stream-format=avc,alignment=au";
pub const AUDIO_CAPS: &str = "audio/mpeg,mpegversion=4,stream-format=raw";
pub const HEVC_CAPS: &str = "video/x-h265,stream-format=hvc1,alignment=au";

/// The first running time either sink saw, so both timelines start at zero
/// together and stay in step.
#[derive(Default)]
pub struct Zero(Mutex<Option<gst::ClockTime>>);

impl Zero {
    fn ms(&self, at: gst::ClockTime) -> u32 {
        let mut zero = self.0.lock().unwrap_or_else(|e| e.into_inner());
        let base = *zero.get_or_insert(at);
        (at.saturating_sub(base).mseconds() & 0xffff_ffff) as u32
    }
}

/// An appsink that hands each H.264 access unit on as a video tag.
pub fn video_sink(to: Shared, zero: Arc<Zero>) -> gst::Element {
    sink(VIDEO_CAPS, TagKind::Video, None, to, zero)
}

/// An appsink that hands each HEVC access unit on as an enhanced RTMP tag.
pub fn hevc_sink(to: Shared, zero: Arc<Zero>) -> gst::Element {
    sink(HEVC_CAPS, TagKind::Video, Some(crate::eflv::HEVC), to, zero)
}

/// An appsink that hands each AAC frame on as an audio tag.
pub fn audio_sink(to: Shared, zero: Arc<Zero>) -> gst::Element {
    sink(AUDIO_CAPS, TagKind::Audio, None, to, zero)
}

/// `cc` is the enhanced RTMP FourCC, for a codec classic FLV has no id for.
fn sink(caps: &str, kind: TagKind, cc: Option<&'static [u8; 4]>, to: Shared, zero: Arc<Zero>) -> gst::Element {
    let caps = caps.parse::<gst::Caps>().expect("the caps above parse");
    let header: Mutex<Option<Vec<u8>>> = Mutex::new(None);
    let sink = gst_app::AppSink::builder()
        .caps(&caps)
        .sync(false)
        .max_buffers(64)
        .drop(true)
        .callbacks(
            gst_app::AppSinkCallbacks::builder()
                .new_sample(move |sink| {
                    let sample = sink.pull_sample().map_err(|_| gst::FlowError::Eos)?;
                    let tags = tags(&sample, kind, cc, &header, &zero);
                    let mut inlet = to.lock().unwrap_or_else(|e| e.into_inner());
                    let Some(inlet) = inlet.as_mut() else { return Err(gst::FlowError::Eos) };
                    for tag in tags {
                        inlet.tag(tag);
                    }
                    Ok(gst::FlowSuccess::Ok)
                })
                .build(),
        )
        .build();
    sink.upcast()
}

/// The tags one sample makes: its sequence header first when the codec
/// configuration is new, then the frame.
fn tags(sample: &gst::Sample, kind: TagKind, cc: Option<&[u8; 4]>, header: &Mutex<Option<Vec<u8>>>, zero: &Zero) -> Vec<MediaTag> {
    let head = |key: bool, hdr: bool, cts: i64| match cc {
        Some(cc) => crate::eflv::prefix(cc, key, hdr, cts),
        None => prefix(kind, key, hdr, cts),
    };
    let Some(buffer) = sample.buffer() else { return Vec::new() };
    let Some(at) = running_time(sample, buffer) else { return Vec::new() };
    let ms = zero.ms(at);
    let mut out = Vec::with_capacity(2);
    if let Some(config) = codec_data(sample) {
        let mut last = header.lock().unwrap_or_else(|e| e.into_inner());
        if last.as_deref() != Some(config.as_slice()) {
            out.push(make(kind, ms, true, true, &head(true, true, 0), &config));
            *last = Some(config);
        }
    }
    let Ok(map) = buffer.map_readable() else { return out };
    let keyframe = kind == TagKind::Video && !buffer.flags().contains(gst::BufferFlags::DELTA_UNIT);
    let cts = match (buffer.pts(), buffer.dts()) {
        (Some(pts), Some(dts)) => pts.mseconds() as i64 - dts.mseconds() as i64,
        _ => 0,
    };
    out.push(make(kind, ms, keyframe, false, &head(keyframe, false, cts), map.as_slice()));
    out
}

/// The bytes RTMP puts in front of a frame: `0x17` or `0x27`, the AVC packet
/// type and a 24 bit composition offset for video; `0xAF` and the AAC packet
/// type for audio.
pub fn prefix(kind: TagKind, keyframe: bool, header: bool, cts_ms: i64) -> Vec<u8> {
    match kind {
        TagKind::Video => {
            let first = if keyframe || header { 0x17 } else { 0x27 };
            let cts = (cts_ms.clamp(-(1 << 23), (1 << 23) - 1) as i32).to_be_bytes();
            vec![first, u8::from(!header), cts[1], cts[2], cts[3]]
        }
        TagKind::Audio => vec![0xAF, u8::from(!header)],
        TagKind::Script => Vec::new(),
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codec;

    #[test]
    fn a_video_prefix_is_what_rtmp_would_have_sent() {
        let key = prefix(TagKind::Video, true, false, 40);
        assert_eq!(key, vec![0x17, 1, 0, 0, 40]);
        assert!(codec::is_keyframe(&key));
        let header = prefix(TagKind::Video, false, true, 0);
        assert!(codec::is_sequence_header(TagKind::Video, &header));
        assert_eq!(codec::video_codec(&header), "h264");
        let inter = prefix(TagKind::Video, false, false, -1);
        assert_eq!(inter, vec![0x27, 1, 0xff, 0xff, 0xff]);
        assert!(!codec::is_keyframe(&inter));
    }

    #[test]
    fn an_audio_prefix_names_aac_and_marks_its_header() {
        let header = prefix(TagKind::Audio, false, true, 0);
        assert!(codec::is_sequence_header(TagKind::Audio, &header));
        assert_eq!(codec::audio_codec(&header), "aac");
        assert!(!codec::is_sequence_header(TagKind::Audio, &prefix(TagKind::Audio, false, false, 0)));
    }

    #[test]
    fn both_timelines_start_at_zero_together() {
        let zero = Zero::default();
        assert_eq!(zero.ms(gst::ClockTime::from_mseconds(5_000)), 0);
        assert_eq!(zero.ms(gst::ClockTime::from_mseconds(5_040)), 40);
        assert_eq!(zero.ms(gst::ClockTime::from_mseconds(4_990)), 0, "never before the start");
    }
}
