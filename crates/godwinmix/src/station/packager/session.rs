//! One connection's pipeline: an appsrc per track into the engine's own
//! HLS packager (`godwinmix_core::hls::attach`), which is what an
//! `hls/output` in a show uses.
//!
//! ```text
//!   tags ─► appsrc (video) ─► queue (leaky) ─► parser ─► cmafmux ─► appsink ─► rung "main"
//!   tags ─► appsrc (sound) ─► queue (leaky) ─► aacparse ─► cmafmux ─► appsink ─► rung "audio"
//! ```
//!
//! Nothing is decoded. `cmafmux` cuts a segment at the first keyframe at or
//! after `segment_ms`, so every segment starts on one of the input's own
//! keyframes. The appsrcs never block the thread that pushes, and the
//! packager's queues are leaky, so a slow muxer loses its own frames.

use super::caps::Read;
use super::flv::{Kind, Tag};
use gstreamer as gst;
use gstreamer::prelude::*;
use gstreamer_app::AppSrc;
use godwinmix_core::hls::{self, Stream, TrackKind};
use std::sync::Arc;

/// The rung a direct show's picture is served as, and its sound.
pub const VIDEO: &str = "main";
pub const AUDIO: &str = "audio";

pub struct Session {
    pipeline: gst::Pipeline,
    video: AppSrc,
    audio: Option<AppSrc>,
    /// The first tag's time: zero on this pipeline's clock.
    base: u32,
}

fn appsrc(caps: &gst::Caps) -> AppSrc {
    let src = AppSrc::builder().caps(caps).format(gst::Format::Time).is_live(true).block(false).max_bytes(8 * 1024 * 1024).build();
    // Full means the oldest frames go, never that the pusher waits.
    src.set_property_from_str("leaky-type", "downstream");
    src
}

impl Session {
    /// Build and start the pipeline, the first frame at `base`. With no
    /// sound caps the output is the picture alone.
    pub fn start(stream: &Arc<Stream>, video: &gst::Caps, audio: Option<&gst::Caps>, base: u32) -> Result<Session, String> {
        let pipeline = gst::Pipeline::with_name(&format!("hls-{}", stream.id));
        let vsrc = appsrc(video);
        let asrc = audio.map(appsrc);
        let add = |src: &AppSrc, id: &str, kind: TrackKind| -> Result<(), String> {
            pipeline.add(src).map_err(|e| e.to_string())?;
            let pad = src.static_pad("src").ok_or("an appsrc with no src pad")?;
            hls::attach(&pipeline, stream, hls::Input { id, kind, pad: &pad, declared_kbps: 0 }).map(|_| ()).map_err(|e| format!("{e:#}"))
        };
        add(&vsrc, VIDEO, TrackKind::Video)?;
        match &asrc {
            Some(a) => add(a, AUDIO, TrackKind::Audio)?,
            None => stream.remove_track(AUDIO),
        }
        if let Err(e) = pipeline.set_state(gst::State::Playing) {
            let _ = pipeline.set_state(gst::State::Null);
            return Err(format!("the HLS packager would not start ({e})"));
        }
        Ok(Session { pipeline, video: vsrc, audio: asrc, base })
    }

    pub fn has_audio(&self) -> bool {
        self.audio.is_some()
    }

    /// Push one frame. Answers the bytes handed on.
    pub fn push(&self, tag: &Tag, read: &Read) -> usize {
        let Read::Frame { skip, cts, key } = *read else { return 0 };
        let src = match tag.kind {
            Kind::Video => &self.video,
            Kind::Audio => match &self.audio {
                Some(a) => a,
                None => return 0,
            },
            Kind::Script => return 0,
        };
        // A frame from before the first keyframe is from before zero.
        let dts = i64::from(tag.ms) - i64::from(self.base);
        let Some(data) = tag.body.get(skip..).filter(|d| !d.is_empty() && dts >= 0) else { return 0 };
        let mut buffer = gst::Buffer::from_slice(data.to_vec());
        if let Some(b) = buffer.get_mut() {
            b.set_dts(gst::ClockTime::from_mseconds(dts as u64));
            b.set_pts(gst::ClockTime::from_mseconds((dts + i64::from(cts)).max(0) as u64));
            if tag.kind == Kind::Video && !key {
                b.set_flags(gst::BufferFlags::DELTA_UNIT);
            }
        }
        // Flushing or full: the frame is lost and the stream goes on.
        let _ = src.push_buffer(buffer);
        data.len()
    }

    /// What the pipeline last said went wrong, without waiting. Every other
    /// message is thrown away as it is read.
    pub fn failure(&self) -> Option<String> {
        let msg = self.pipeline.bus()?.pop_filtered(&[gst::MessageType::Error])?;
        match msg.view() {
            gst::MessageView::Error(e) => Some(format!("{} ({})", e.error(), msg.src().map(|s| s.name().to_string()).unwrap_or_default())),
            _ => None,
        }
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        let _ = self.pipeline.set_state(gst::State::Null);
    }
}
