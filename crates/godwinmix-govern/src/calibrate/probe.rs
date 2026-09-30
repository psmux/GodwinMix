//! The individual runs: the bare source, each encoder at each shape and
//! preset, a scale, a decode, an audio encode.

use super::pipeline::{make, run, sink, video_source};
use super::{AudioCandidate, Candidate, Options};
use crate::calibration::{mpix, AudioCal, EncoderCal, Point, PresetPoint};
use godwinmix_protocol::rendition::{Fps, VideoShape};
use gstreamer as gst;
use gstreamer::glib;
use gstreamer::prelude::*;
use std::time::{Duration, Instant};

const PATIENCE: Duration = Duration::from_secs(10);

type Shape = (u32, u32, u32);

/// Where the encoder sits in `encode_chain`: source, caps, convert, encoder.
const ENCODER: usize = 3;

/// The test source's own cost per shape, taken off every run at that shape.
pub struct Baselines(Vec<(Shape, (f64, f64))>);

impl Baselines {
    fn get(&self, s: Shape) -> (f64, f64) {
        self.0.iter().find(|(k, _)| *k == s).map(|(_, v)| *v).unwrap_or((0.0, 0.0))
    }
}

pub fn baselines(shapes: &[Shape], frames: u32, notes: &mut Vec<String>) -> Baselines {
    let mut out = Vec::new();
    for &s in shapes {
        let els = video_source(s.0, s.1, s.2, frames).and_then(|mut v| {
            v.push(sink()?);
            Ok(v)
        });
        match els.and_then(|e| run(&e, e.len() - 1, PATIENCE)).map(|t| t.per_second()) {
            Ok(Some(v)) => out.push((s, v)),
            Ok(None) | Err(_) => notes.push(format!("the bare test source at {}x{} could not be timed", s.0, s.1)),
        }
    }
    Baselines(out)
}

fn shape_of(c: &Candidate, s: Shape) -> VideoShape {
    // A live bitrate for the size: 6 Mbit/s at 1080p30, in proportion below.
    let kbps = (6000.0 * mpix(s.0, s.1, Fps::whole(s.2)) / mpix(1920, 1080, Fps::whole(30))).round() as u32;
    VideoShape { codec: c.slot.codec, width: s.0, height: s.1, fps: Fps::whole(s.2), bitrate_kbps: kbps.max(500), keyframe_ms: 2000 }
}

/// Source, convert, the configured encoder, and optionally more after it.
fn encode_chain(c: &Candidate, s: Shape, frames: u32, preset: Option<&str>, tail: &[&str]) -> Result<Vec<gst::Element>, String> {
    let mut v = video_source(s.0, s.1, s.2, frames)?;
    v.push(make("videoconvert")?);
    let enc = make(&c.element)?;
    (c.configure)(&enc, &shape_of(c, s));
    if let Some(p) = preset {
        enc.set_property_from_str("speed-preset", p);
    }
    v.push(enc);
    for t in tail {
        v.push(make(t)?);
    }
    v.push(sink()?);
    Ok(v)
}

/// One timed run from the element at `start` on, less the source, per
/// second of media.
fn timed(els: Result<Vec<gst::Element>, String>, start: usize, base: (f64, f64)) -> Result<(u32, u32), String> {
    let t = run(&els?, start, PATIENCE)?;
    let (cpu, wall) = t.per_second().ok_or("too few frames came out to time")?;
    Ok(((cpu - base.0).max(1.0).round() as u32, (wall - base.1).max(1.0).round() as u32))
}

pub fn encoder(c: &Candidate, base: &Baselines, opts: &Options, started: Instant, notes: &mut Vec<String>) -> Option<EncoderCal> {
    let mut points = Vec::new();
    for &s in &opts.shapes {
        match timed(encode_chain(c, s, opts.frames, None, &[]), ENCODER, base.get(s)) {
            Ok((cpu, wall)) => points.push(Point { width: s.0, height: s.1, fps: Fps::whole(s.2), cpu_millicores: cpu, wall_ms: wall }),
            Err(e) => {
                notes.push(format!("{} ({}) could not encode {}x{}: {e}", c.slot.id, c.element, s.0, s.1));
                return None;
            }
        }
    }
    let (preset, presets) = presets(c, base, opts, started, points.last()?, notes);
    Some(EncoderCal { slot: c.slot.clone(), element: c.element.clone(), preset, points, presets, sessions: None })
}

/// The configured preset, and the others timed at the largest shape,
/// fastest first. Slower ones stop once a preset would not run in real time
/// on the whole machine.
fn presets(c: &Candidate, base: &Baselines, opts: &Options, started: Instant, top: &Point, notes: &mut Vec<String>) -> (Option<String>, Vec<PresetPoint>) {
    let Ok(enc) = make(&c.element) else { return (None, Vec::new()) };
    let Some(known) = preset_nicks(&enc) else { return (None, Vec::new()) };
    (c.configure)(&enc, &shape_of(c, (top.width, top.height, top.fps.num)));
    let configured = glib::EnumValue::from_value(&enc.property_value("speed-preset")).map(|(_, v)| v.nick().to_string());
    let limit = crate::fingerprint::cores() * 1000;
    let s = (top.width, top.height, top.fps.num);
    let mut out = Vec::new();
    for nick in known.iter().filter(|n| opts.presets.contains(n) || Some(*n) == configured.as_ref()) {
        if Some(nick) == configured.as_ref() {
            out.push(PresetPoint { preset: nick.clone(), point: top.clone() });
            continue;
        }
        if started.elapsed() > opts.deadline {
            notes.push(format!("{} preset {nick} not timed: calibration is kept short", c.slot.id));
            continue;
        }
        match timed(encode_chain(c, s, opts.frames, Some(nick), &[]), ENCODER, base.get(s)) {
            Ok((cpu, wall)) => {
                out.push(PresetPoint { preset: nick.clone(), point: Point { cpu_millicores: cpu, wall_ms: wall, ..top.clone() } });
                if cpu > limit {
                    break;
                }
            }
            Err(e) => notes.push(format!("{} preset {nick}: {e}", c.slot.id)),
        }
    }
    (configured, out)
}

/// The `speed-preset` nicks this element knows, fastest first (the enum's
/// own order, as x264 and x265 define it).
fn preset_nicks(el: &gst::Element) -> Option<Vec<String>> {
    let spec = el.find_property("speed-preset")?;
    let e = spec.downcast_ref::<glib::ParamSpecEnum>()?;
    let mut v: Vec<(i32, String)> = e.enum_class().values().iter().map(|v| (v.value(), v.nick().to_string())).collect();
    v.sort();
    Some(v.into_iter().filter(|(n, _)| *n > 0).map(|(_, s)| s).collect())
}

pub fn scale(base: &Baselines, frames: u32, notes: &mut Vec<String>) -> Option<f64> {
    let (from, to) = ((1920, 1080, 30), (1280, 720, 30));
    let els = (|| {
        let mut v = video_source(from.0, from.1, from.2, frames)?;
        v.push(make("videoscale")?);
        v.push(make("videoconvert")?);
        let f = make("capsfilter")?;
        f.set_property("caps", gst::Caps::builder("video/x-raw").field("format", "NV12").field("width", to.0 as i32).field("height", to.1 as i32).build());
        v.push(f);
        v.push(sink()?);
        Ok(v)
    })();
    match timed(els, 2, base.get(from)) {
        Ok((cpu, _)) => Some(f64::from(cpu) / (mpix(from.0, from.1, Fps::whole(from.2)) + mpix(to.0, to.1, Fps::whole(to.2)))),
        Err(e) => {
            notes.push(format!("scaling could not be timed: {e}"));
            None
        }
    }
}

/// Encode, parse and decode at `point`'s shape: the whole, in thousandths.
pub fn round_trip(c: &Candidate, point: &Point, base: &Baselines, frames: u32, notes: &mut Vec<String>) -> Option<u32> {
    let s = (point.width, point.height, point.fps.num);
    let tail = [c.parser.as_deref()?, c.decoder.as_deref()?];
    match timed(encode_chain(c, s, frames, None, &tail), ENCODER, base.get(s)) {
        Ok((cpu, _)) => Some(cpu),
        Err(e) => {
            notes.push(format!("decoding {} could not be timed: {e}", c.slot.id));
            None
        }
    }
}

pub fn audio(a: &AudioCandidate, notes: &mut Vec<String>) -> Option<AudioCal> {
    let els = (|| {
        let src = make("audiotestsrc")?;
        src.set_property("num-buffers", 50i32);
        src.set_property("samplesperbuffer", 960i32);
        let f = make("capsfilter")?;
        f.set_property("caps", gst::Caps::builder("audio/x-raw").field("format", "S16LE").field("rate", 48000i32).field("channels", 2i32).field("layout", "interleaved").build());
        let enc = make(&a.element)?;
        (a.configure)(&enc);
        Ok(vec![src, f, make("audioconvert")?, make("audioresample")?, enc, sink()?])
    })();
    match timed(els, 4, (0.0, 0.0)) {
        Ok((cpu, _)) => Some(AudioCal { id: a.id.clone(), codec: a.codec, element: a.element.clone(), cpu_millicores: cpu }),
        Err(e) => {
            notes.push(format!("{} ({}) could not be timed: {e}", a.id, a.element));
            None
        }
    }
}
