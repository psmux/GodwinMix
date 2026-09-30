//! Calibration: a second of synthetic 720p30 and 1080p30 through each
//! encoder this machine has, timed, and the hardware session limit where
//! the device will say.
//!
//! The encoders come in as [`Candidate`]s, built by the caller from the
//! codec catalogue with the catalogue's own presence check and property
//! code, so there is one list of encoders and one way to configure them.
//! This module adds only the timing.
//!
//! How a run is timed: CPU (this process, every thread) and wall time are
//! read when the first frame enters the encoder and when the last one leaves
//! the pipeline, so element start up is left out and an encoder that holds
//! frames back is still timed for all of them. The test source's own cost,
//! timed with no encoder, is taken off. Everything runs one after another,
//! since process CPU cannot tell two pipelines apart.

mod extra;
mod pipeline;
mod probe;
mod sessions;

use crate::calibration::{mpix, AudioCal, Calibration, DecoderCal, EncoderCal, FORMAT, SOFTWARE_MARGIN};
use crate::fingerprint::{fingerprint, Machine};
use godwinmix_protocol::rendition::{AudioCodec, EncoderSlot, VideoShape};
use gstreamer as gst;
use std::sync::Arc;
use std::time::{Duration, Instant};

pub type Configure = Arc<dyn Fn(&gst::Element, &VideoShape) + Send + Sync>;
pub type ConfigureAudio = Arc<dyn Fn(&gst::Element) + Send + Sync>;

/// One encoder to measure.
#[derive(Clone)]
pub struct Candidate {
    pub slot: EncoderSlot,
    pub element: String,
    /// Parser and software decoder for the codec, so decoding it can be
    /// timed too. The first software candidate of a codec that has both is
    /// used.
    pub parser: Option<String>,
    pub decoder: Option<String>,
    /// Sets the catalogue's properties for this shape.
    pub configure: Configure,
}

#[derive(Clone)]
pub struct AudioCandidate {
    pub id: String,
    pub codec: AudioCodec,
    pub element: String,
    pub configure: ConfigureAudio,
}

#[derive(Debug, Clone)]
pub struct Options {
    /// Shapes timed for every encoder, smallest first.
    pub shapes: Vec<(u32, u32, u32)>,
    /// Frames per run: 30 is a second at 30 fps.
    pub frames: u32,
    /// Speed presets tried on an encoder with a `speed-preset` property,
    /// fastest first. Only those the element knows are run.
    pub presets: Vec<String>,
    /// Hardware sessions to open before calling a device unlimited.
    pub session_cap: u32,
    /// Optional runs (other presets, decode, audio) are skipped past this.
    pub deadline: Duration,
}

impl Default for Options {
    fn default() -> Self {
        Options {
            shapes: vec![(1280, 720, 30), (1920, 1080, 30)],
            frames: 30,
            presets: ["ultrafast", "superfast", "veryfast", "faster"].map(String::from).to_vec(),
            session_cap: 8,
            deadline: Duration::from_secs(8),
        }
    }
}

/// The fingerprint these candidates would be stored under on this machine.
pub fn fingerprint_for(candidates: &[Candidate]) -> String {
    let _ = gst::init();
    let names: Vec<String> = candidates.iter().map(|c| format!("{}={}", c.slot.id, c.element)).collect();
    fingerprint(&Machine::current(), &names, &gst::version_string())
}

/// Measure this machine. Blocking, a few seconds; never call it on a
/// streaming thread or while something is on air unless a person asked.
pub fn calibrate(candidates: &[Candidate], audio: &[AudioCandidate], opts: &Options) -> Calibration {
    let started = Instant::now();
    let mut cal = Calibration {
        format: FORMAT,
        fingerprint: fingerprint_for(candidates),
        machine: Machine::current(),
        software_margin: SOFTWARE_MARGIN,
        taken_unix: std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0),
        ..Default::default()
    };
    if let Err(e) = gst::init() {
        cal.notes.push(format!("GStreamer did not start: {e}"));
        return cal;
    }
    let baselines = probe::baselines(&opts.shapes, opts.frames, &mut cal.notes);
    for c in candidates {
        match probe::encoder(c, &baselines, opts, started, &mut cal.notes) {
            Some(e) => cal.encoders.push(e),
            None => continue,
        }
    }
    sessions::probe_all(candidates, &mut cal.encoders, opts.session_cap, &mut cal.notes);
    let over = |cal: &mut Calibration, what: &str| {
        let late = started.elapsed() > opts.deadline;
        if late {
            cal.notes.push(format!("{what} not timed: calibration is kept under {} s", opts.deadline.as_secs()));
        }
        late
    };
    if !over(&mut cal, "scaling") {
        cal.scale_per_mpix = extra::scale(&baselines, opts.frames, &mut cal.notes);
    }
    if !over(&mut cal, "decoding") {
        cal.decoders = decoders(candidates, &cal.encoders, &baselines, opts, &mut cal.notes);
    }
    if !over(&mut cal, "audio") {
        cal.audio = audio.iter().filter_map(|a| extra::audio(a, &mut cal.notes)).collect::<Vec<AudioCal>>();
    }
    cal.took_ms = started.elapsed().as_millis() as u64;
    cal
}

/// Decode cost per codec, from the first software candidate with a parser
/// and a decoder, at the largest shape.
fn decoders(candidates: &[Candidate], encs: &[EncoderCal], base: &probe::Baselines, opts: &Options, notes: &mut Vec<String>) -> Vec<DecoderCal> {
    let mut out: Vec<DecoderCal> = Vec::new();
    for c in candidates.iter().filter(|c| !c.slot.hardware && c.parser.is_some() && c.decoder.is_some()) {
        if out.iter().any(|d| d.codec == c.slot.codec) {
            continue;
        }
        let Some(enc) = encs.iter().find(|e| e.slot.id == c.slot.id) else { continue };
        let Some(point) = enc.points.last() else { continue };
        let Some(total) = extra::round_trip(c, point, base, opts.frames, notes) else { continue };
        let decode = total.saturating_sub(point.cpu_millicores);
        if decode == 0 {
            notes.push(format!("decoding {} cost less than the timing can see; the cautious figure is used", c.slot.id));
            continue;
        }
        let per_mpix = f64::from(decode) / mpix(point.width, point.height, point.fps);
        out.push(DecoderCal { codec: c.slot.codec, element: c.decoder.clone().unwrap_or_default(), per_mpix });
    }
    out
}
