//! One connection's pipeline: an appsrc per track into the engine's own
//! HLS packager (`godwinmix_core::hls::attach`), which is what an
//! `hls/output` in a show uses.
//!
//! ```text
//!   tags ─► appsrc (video) ─► queue (leaky) ─► parser ─► cmafmux ─► appsink ─► rung "main"
//!   tags ─► appsrc (sound) ─► queue (leaky) ─► aacparse ─► cmafmux ─► appsink ─► rung "audio", "audio1" ...
//!   tags ─► ts::Cutters (when `segments` asks for MPEG-TS) ─► "main", "audio1" ...
//! ```
//!
//! Nothing is decoded. `cmafmux` cuts a segment at the first keyframe at or
//! after `segment_ms`, so every segment starts on one of the input's own
//! keyframes. The appsrcs never block the thread that pushes, and the
//! packager's queues are leaky, so a slow muxer loses its own frames.

use super::caps::Read;
use super::flv::{Kind, Tag};
use super::tracks::{self, Sound};
use super::ts::Cutters;
use gstreamer as gst;
use gstreamer::prelude::*;
use gstreamer_app::AppSrc;
use godwinmix_core::hls::{self, Stream, TrackKind};
use std::sync::Arc;

/// The rung a direct show's picture is served as; its sounds are `tracks::rung`.
pub const VIDEO: &str = "main";

pub struct Session {
    pipeline: gst::Pipeline,
    video: AppSrc,
    /// One appsrc per sound track packaged as CMAF, by track.
    audio: Vec<(u8, AppSrc)>,
    /// Every sound track this session was built for, CMAF or not.
    sounds: Vec<u8>,
    ts: Option<Cutters>,
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
    /// Build and start the pipeline, the first frame at `base`: the picture,
    /// and each sound track in `sounds` (track, caps) as a rung of its own,
    /// named and tagged from `named`. With no sound the output is the
    /// picture alone. MPEG-TS renditions are cut beside it when the output
    /// asks for them.
    pub fn start(stream: &Arc<Stream>, video: &gst::Caps, sounds: &[(u8, gst::Caps)], base: u32, named: &[Sound]) -> Result<Session, String> {
        // The connection before this one is gone with its pipeline, so the
        // timestamps start again: every track marks the break alike.
        stream.restart();
        // Zero on this pipeline's clock is now, when the first tag came, and
        // not whenever the pipeline gets to playing: building it can take a
        // second on a busy machine, and every frame would then be that much
        // ahead of the clock.
        let clock = gst::SystemClock::obtain();
        let zero = clock.time();
        let pipeline = gst::Pipeline::with_name(&format!("hls-{}", stream.id));
        pipeline.use_clock(Some(&clock));
        pipeline.set_start_time(gst::ClockTime::NONE);
        pipeline.set_base_time(zero);
        let add = |src: &AppSrc, id: &str, kind: TrackKind| -> Result<(), String> {
            pipeline.add(src).map_err(|e| e.to_string())?;
            let pad = src.static_pad("src").ok_or("an appsrc with no src pad")?;
            hls::attach(&pipeline, stream, hls::Input { id, kind, pad: &pad, declared_kbps: 0 }).map(|_| ()).map_err(|e| format!("{e:#}"))
        };
        let vsrc = appsrc(video);
        add(&vsrc, VIDEO, TrackKind::Video)?;
        let cmaf = stream.params.segments.cmaf();
        let mut audio = Vec::new();
        for (track, caps) in sounds.iter().filter(|_| cmaf) {
            let (src, rung) = (appsrc(caps), tracks::rung(*track));
            add(&src, &rung, TrackKind::Audio)?;
            let lang = named.iter().find(|s| s.track == *track).map(|s| s.lang.clone()).unwrap_or_default();
            if let Some(t) = stream.track(&rung) {
                t.update_info(|i| (i.sound, i.lang) = (*track, lang));
            }
            audio.push((*track, src));
        }
        for t in stream.tracks().iter().filter(|t| t.kind == TrackKind::Audio && !audio.iter().any(|(n, _)| tracks::rung(*n) == t.id)) {
            stream.remove_track(&t.id);
        }
        let ts = stream.params.segments.ts().then(|| Cutters::new(stream, video, sounds, base, named));
        if let Err(e) = pipeline.set_state(gst::State::Playing) {
            let _ = pipeline.set_state(gst::State::Null);
            return Err(format!("the HLS packager would not start ({e})"));
        }
        let sounds = sounds.iter().map(|(t, _)| *t).collect();
        Ok(Session { pipeline, video: vsrc, audio, sounds, ts, base })
    }

    /// Whether this session was built with sound track `track`.
    pub fn has_sound(&self, track: u8) -> bool { self.sounds.contains(&track) }

    /// Hand on one tag: a frame to its CMAF appsrc, and anything to its TS
    /// cutter. Answers the bytes handed on.
    pub fn push(&mut self, tag: &Tag, read: &Read) -> usize {
        if let Some(ts) = self.ts.as_mut() {
            ts.push(tag, read);
        }
        let Read::Frame { skip, cts, key } = *read else { return 0 };
        let src = match tag.kind {
            Kind::Video => &self.video,
            Kind::Audio => match self.audio.iter().find(|(t, _)| *t == tag.track) {
                Some((_, a)) => a,
                None => return 0,
            },
            Kind::Script => return 0,
        };
        // A frame from before the first keyframe is from before zero.
        let dts = i64::from(tag.ms) - i64::from(self.base);
        if tag.body.len() <= skip || dts < 0 {
            return 0;
        }
        // The relay's own bytes, not a copy: the TS cutter holds the same.
        let mut buffer = gst::Buffer::from_slice(tag.body.slice(skip..));
        if let Some(b) = buffer.get_mut() {
            b.set_dts(gst::ClockTime::from_mseconds(dts as u64));
            b.set_pts(gst::ClockTime::from_mseconds((dts + i64::from(cts)).max(0) as u64));
            if tag.kind == Kind::Video && !key {
                b.set_flags(gst::BufferFlags::DELTA_UNIT);
            }
        }
        // Flushing or full: the frame is lost and the stream goes on.
        let _ = src.push_buffer(buffer);
        tag.body.len() - skip
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
