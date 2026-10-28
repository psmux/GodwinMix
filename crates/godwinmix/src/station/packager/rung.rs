//! A ladder's further picture: one more connection to the relay, read for
//! its picture alone, packaged as one more video track of the output's
//! stream, so the master lists it beside the first.
//!
//! ```text
//!   relay "<show>/main|<rung video>|<audio>" ─► appsrc ─► parser ─► cmafmux ─► track "main-720p"
//!                                            └► TsCut, picture and first sound ─► TS "main-720p"
//! ```
//!
//! The sound is the first rung's (`session.rs`); a rung's own copy of it
//! goes only into its MPEG-TS, where the first sound rides with the
//! picture. Every rung's frames are timed from the first rung's zero
//! ([`Zero`]), so the variants of one moment carry one time and a player
//! moves between them without a jump. When the first rung starts again its
//! zero moves, and every rung starts again on it.

use super::caps::{self, Read};
use super::flv::{self, Kind, Tag};
use super::ts;
use godwinmix_core::hls::{self, Stream, TrackKind};
use gstreamer as gst;
use gstreamer::prelude::*;
use gstreamer_app::AppSrc;
use parking_lot::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

/// The first rung's zero: which start of its pipeline, and the relay time
/// that start is zero at. None before it has started.
#[derive(Default)]
pub struct Zero(Mutex<Option<(u64, u32)>>);

impl Zero {
    /// The first rung started again, at relay time `base`.
    pub fn moved(&self, base: u32) {
        let mut z = self.0.lock();
        let next = z.map_or(1, |(n, _)| n + 1);
        *z = Some((next, base));
    }

    fn now(&self) -> Option<(u64, u32)> {
        *self.0.lock()
    }
}

/// How a rung's connection ended.
enum End {
    Stopped,
    /// Lost, or the first rung moved its zero: connect and start again.
    Again,
}

/// Package one rung until `stop`, connecting again whenever it is lost.
pub fn run(rung: &super::wire::Rung, stream: &Arc<Stream>, zero: &Zero, stop: &AtomicBool) {
    while !stop.load(Ordering::Relaxed) {
        let end = match flv::Reader::open(rung.source.relay, &rung.source.path, Duration::from_secs(5)) {
            Ok(reader) => follow(reader, &rung.id, stream, zero, stop),
            Err(_) => End::Again,
        };
        if matches!(end, End::Stopped) {
            return;
        }
        let until = std::time::Instant::now() + Duration::from_secs(1);
        while !stop.load(Ordering::Relaxed) && std::time::Instant::now() < until {
            std::thread::sleep(Duration::from_millis(50));
        }
    }
}

/// What one connection has learned, and its pipeline once it has one.
#[derive(Default)]
struct Learned {
    video: Option<gst::Caps>,
    sound: Option<gst::Caps>,
    running: Option<(u64, Pipeline)>,
}

fn follow(mut reader: flv::Reader, id: &str, stream: &Arc<Stream>, zero: &Zero, stop: &AtomicBool) -> End {
    let mut at = Learned::default();
    loop {
        if stop.load(Ordering::Relaxed) {
            return End::Stopped;
        }
        let tag = match reader.ready(Duration::from_secs(30), stop).and_then(|go| go.then(|| reader.next()).transpose()) {
            Ok(Some(t)) => t,
            Ok(None) => return End::Stopped,
            Err(_) => return End::Again,
        };
        let read = match tag.kind {
            Kind::Video => caps::video(&tag.body),
            Kind::Audio if tag.track == 0 => caps::audio(&tag.body),
            _ => continue,
        };
        let gen = zero.now();
        if let (Some((was, _)), Some((now, _))) = (&at.running, gen) {
            if *was != now {
                return End::Again;
            }
        }
        match (&read, tag.kind) {
            (Read::Header(c), Kind::Video) => {
                if at.video.as_ref().is_some_and(|v| v != c) && at.running.is_some() {
                    return End::Again;
                }
                at.video = Some(c.clone());
            }
            (Read::Header(c), Kind::Audio) => at.sound = Some(c.clone()),
            (Read::Frame { key: true, .. }, Kind::Video) if at.running.is_none() => {
                let (Some(video), Some((n, base))) = (at.video.as_ref(), gen) else { continue };
                // From the first rung's own first keyframe on, never before.
                if tag.ms < base {
                    continue;
                }
                match Pipeline::start(id, stream, video, at.sound.as_ref(), base) {
                    Ok(p) => at.running = Some((n, p)),
                    Err(_) => return End::Again,
                }
            }
            _ => {}
        }
        if let Some((_, p)) = at.running.as_mut() {
            p.push(&tag, &read);
        }
    }
}

/// One rung's pipeline: its picture as CMAF, and its MPEG-TS beside it when
/// the output makes TS.
struct Pipeline {
    pipeline: gst::Pipeline,
    video: AppSrc,
    ts: Option<ts::Cutters>,
    base: u32,
}

impl Pipeline {
    fn start(id: &str, stream: &Arc<Stream>, video: &gst::Caps, sound: Option<&gst::Caps>, base: u32) -> Result<Pipeline, String> {
        let pipeline = gst::Pipeline::with_name(&format!("hls-{}-{id}", stream.id));
        let src = super::session::appsrc(video);
        pipeline.add(&src).map_err(|e| e.to_string())?;
        let pad = src.static_pad("src").ok_or("an appsrc with no src pad")?;
        hls::attach(&pipeline, stream, hls::Input { id, kind: TrackKind::Video, pad: &pad, declared_kbps: 0 }).map_err(|e| format!("{e:#}"))?;
        let sounds: Vec<(u8, gst::Caps)> = sound.map(|c| (0, c.clone())).into_iter().collect();
        let ts = stream.params.segments.ts().then(|| ts::Cutters::rung(stream, id, video, &sounds, base));
        pipeline.set_state(gst::State::Playing).map_err(|e| e.to_string())?;
        Ok(Pipeline { pipeline, video: src, ts, base })
    }

    fn push(&mut self, tag: &Tag, read: &Read) {
        if let Some(ts) = self.ts.as_mut() {
            ts.push(tag, read);
        }
        if tag.kind != Kind::Video {
            return;
        }
        if let Some(buffer) = super::session::buffer(tag, read, self.base) {
            let _ = self.video.push_buffer(buffer);
        }
    }
}

impl Drop for Pipeline {
    fn drop(&mut self) {
        let _ = self.pipeline.set_state(gst::State::Null);
    }
}
