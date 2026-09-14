//! `gmx codec test`: does this entry actually encode on this machine?
//!
//! A driver that loads is not a driver that works. The registry says
//! `nvh264enc` is there, the element builds, the pipeline goes to PLAYING and
//! then nothing comes out, or what comes out is garbage. That is a thing to
//! find out on a Tuesday afternoon and not ten minutes before a show, so this
//! runs the real round trip: the test pattern in, the entry's encoder, its
//! parser, its decoder, and a frame by frame comparison against the source.
//!
//! What it measures:
//!
//! * frame count in and out, because a hardware encoder that drops half the
//!   frames still produces a valid stream,
//! * PSNR of the luma plane against the source frame it came from, because a
//!   decoder that outputs green is a valid stream too,
//! * CPU seconds and peak RSS over the run, which is the number that goes in
//!   the `verified` report and the number a Pi owner wants before buying.
//!
//! The comparison streams: the reference side blocks when it gets more than a
//! few frames ahead, so a ten second 720p test holds a handful of frames and
//! not three hundred.

use super::apply::{self, Vars};
use super::model::{Keyframe, PropValue, Role};
use super::select::Registry;
use super::Catalogue;
use crate::gstutil::{capsfilter, make, queue_thread};
use anyhow::{bail, Context, Result};
use gstreamer as gst;
use gstreamer::prelude::*;
use gstreamer_app as gst_app;
use gstreamer_video as gst_video;
use gstreamer_video::prelude::*;
use parking_lot::{Condvar, Mutex};
use serde::Serialize;
use std::collections::BTreeMap;
use std::collections::VecDeque;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// How far the reference side may run ahead of the round trip side. Small
/// enough that the test holds megabytes rather than hundreds of them, large
/// enough to cover any encoder's pipelining.
const LOOKAHEAD: usize = 12;

/// How long either side waits for the other before giving up and calling the
/// run failed. An encoder that has stopped producing is the fault being
/// looked for, so this must not be infinite.
const PATIENCE: Duration = Duration::from_secs(10);

/// PSNR below which a round trip is called broken rather than merely lossy.
/// A working 4 Mbit/s H.264 round trip of the test pattern sits in the high
/// thirties; anything under thirty is a different picture, not a compressed
/// one.
pub const PSNR_FLOOR: f64 = 30.0;

#[derive(Debug, Clone, Serialize)]
pub struct CodecReport {
    pub entry: String,
    pub kind: String,
    pub codec: String,
    pub accel: String,
    pub encoder: String,
    pub decoder: Option<String>,
    pub width: i32,
    pub height: i32,
    pub fps: i32,
    pub seconds: f64,
    pub frames_in: u64,
    pub frames_out: u64,
    pub psnr_db: Option<f64>,
    pub min_psnr_db: Option<f64>,
    pub cpu_secs: f64,
    pub cpu_percent: f64,
    pub rss_mb: f64,
    pub wall_secs: f64,
    pub platform: String,
    pub gstreamer: String,
    pub ok: bool,
    pub note: String,
}

impl CodecReport {
    /// The report in the shape it goes into the catalogue's `verified` list,
    /// ready to paste into a pull request.
    pub fn verified_toml(&self) -> String {
        let psnr = match self.psnr_db {
            Some(p) => format!("psnr {p:.1} dB, "),
            None => String::new(),
        };
        format!(
            "verified = [{{ platform = \"{}\", driver = \"fill in\", gstreamer = \"{}\", \
             by = \"fill in\", date = \"{}\", report = \"{}{}/{} frames, {:.2} core at real time, {:.0} MB RSS\" }}]",
            self.platform,
            self.gstreamer,
            today(),
            psnr,
            self.frames_out,
            self.frames_in,
            self.cpu_percent / 100.0,
            self.rss_mb,
        )
    }

    pub fn human(&self) -> String {
        let mut s = format!(
            "{}  {} -> {}\n  {}x{}@{} for {:.0} s\n  frames    {} in, {} out\n",
            self.entry,
            self.encoder,
            self.decoder.as_deref().unwrap_or("(no decoder)"),
            self.width,
            self.height,
            self.fps,
            self.seconds,
            self.frames_in,
            self.frames_out,
        );
        if let (Some(avg), Some(min)) = (self.psnr_db, self.min_psnr_db) {
            s.push_str(&format!(
                "  psnr      {avg:.1} dB average, {min:.1} dB worst frame\n"
            ));
        }
        s.push_str(&format!(
            "  cpu       {:.2} s for {:.0} s of media, {:.2} of one core at real time\n\
             \x20 wall      {:.1} s (the test runs as fast as it can, not in real time)\n\
             \x20 rss       {:.0} MB peak, whole process\n  result    {}\n",
            self.cpu_secs,
            self.seconds,
            self.cpu_percent / 100.0,
            self.wall_secs,
            self.rss_mb,
            if self.ok { "pass" } else { self.note.as_str() }
        ));
        s
    }
}

/// Today as `YYYY-MM-DD`, UTC. Written out rather than pulled in, because one
/// date in one report is not worth a dependency. Civil from days, Howard
/// Hinnant's algorithm.
fn today() -> String {
    let Ok(d) = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH) else {
        return "unknown".into();
    };
    let z = (d.as_secs() / 86_400) as i64 + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!("{year:04}-{month:02}-{day:02}")
}

/// Run the round trip for one catalogue entry.
pub fn test_entry(
    cat: &Catalogue,
    id: &str,
    seconds: f64,
    width: i32,
    height: i32,
    fps: i32,
) -> Result<CodecReport> {
    if let Some(e) = cat.video_entry(id) {
        let e = e.clone();
        let Some(encoder) = e.encoder.clone() else {
            bail!("{id} is a decode only entry: there is nothing to encode with");
        };
        return video_round_trip(
            &Spec {
                id: id.into(),
                kind: "video".into(),
                codec: e.codec.clone(),
                accel: e.accel.clone(),
                encoder,
                parser: e.parser.clone(),
                decoder: e.decoder.clone(),
                download: e.download.clone(),
                properties: e.properties.clone(),
                keyframe: e.keyframe.clone(),
            },
            seconds,
            width,
            height,
            fps,
        );
    }
    if let Some(e) = cat.audio_entry(id) {
        let e = e.clone();
        let Some(encoder) = e.encoder.clone() else {
            bail!("{id} is a decode only entry: there is nothing to encode with");
        };
        return audio_round_trip(
            id,
            &e.codec,
            &encoder,
            e.parser.as_deref(),
            e.decoder.as_deref(),
            &e.properties,
            seconds,
        );
    }
    if cat.graphics_entry(id).is_some() {
        bail!("{id} is a graphics entry. Force it with [hardware] graphics = \"{id}\" and watch the programme; there is no encode to test.");
    }
    let known: Vec<String> = cat
        .video
        .iter()
        .map(|e| e.id())
        .chain(cat.audio.iter().map(|e| e.id()))
        .collect();
    bail!(
        "no catalogue entry called {id:?}. Known entries: {}",
        known.join(", ")
    )
}

struct Spec {
    id: String,
    kind: String,
    codec: String,
    accel: String,
    encoder: String,
    parser: Option<String>,
    decoder: Option<String>,
    download: Option<String>,
    properties: BTreeMap<String, PropValue>,
    keyframe: Option<Keyframe>,
}

#[derive(Default)]
struct Tally {
    queue: VecDeque<Vec<u8>>,
    abort: bool,
    frames_in: u64,
    frames_out: u64,
    compared: u64,
    psnr_sum: f64,
    psnr_min: f64,
    mismatched: u64,
}

struct Shared {
    tally: Mutex<Tally>,
    space: Condvar,
    filled: Condvar,
}

fn video_round_trip(
    spec: &Spec,
    seconds: f64,
    width: i32,
    height: i32,
    fps: i32,
) -> Result<CodecReport> {
    let frames = (seconds * fps as f64).round() as i32;
    let shared = Arc::new(Shared {
        tally: Mutex::new(Tally {
            psnr_min: f64::INFINITY,
            ..Default::default()
        }),
        space: Condvar::new(),
        filled: Condvar::new(),
    });
    let pipeline = build_video_pipeline(spec, &shared, frames, width, height, fps)?;

    let before = resources();
    let started = Instant::now();
    let outcome = run_to_eos(&pipeline, Duration::from_secs_f64(seconds * 3.0 + 20.0));
    let wall = started.elapsed().as_secs_f64();
    {
        shared.tally.lock().abort = true;
    }
    shared.space.notify_all();
    shared.filled.notify_all();
    let _ = pipeline.set_state(gst::State::Null);
    let after = resources();

    let t = shared.tally.lock();
    let avg = if t.compared > 0 {
        Some(t.psnr_sum / t.compared as f64)
    } else {
        None
    };
    let min = if t.compared > 0 {
        Some(t.psnr_min)
    } else {
        None
    };
    let cpu = (after.0 - before.0).max(0.0);
    let mut note = match outcome {
        Ok(()) => String::new(),
        Err(e) => format!("{e:#}"),
    };
    let mut ok = note.is_empty();
    if ok && t.frames_out == 0 {
        ok = false;
        note = "the pipeline ran but no frame came back out of the decoder".into();
    }
    if ok && t.frames_out * 20 < t.frames_in * 19 {
        ok = false;
        note = format!(
            "{} of {} frames came back: this entry is dropping frames",
            t.frames_out, t.frames_in
        );
    }
    if ok {
        if let Some(avg) = avg {
            if avg < PSNR_FLOOR {
                ok = false;
                note = format!("psnr {avg:.1} dB is below the {PSNR_FLOOR:.0} dB floor: the picture that came back is not the one that went in");
            }
        }
    }
    if ok && t.mismatched > 0 {
        note = format!(
            "{} frames came back at a different size and were not compared",
            t.mismatched
        );
    }
    Ok(CodecReport {
        entry: spec.id.clone(),
        kind: spec.kind.clone(),
        codec: spec.codec.clone(),
        accel: spec.accel.clone(),
        encoder: spec.encoder.clone(),
        decoder: spec.decoder.clone(),
        width,
        height,
        fps,
        seconds,
        frames_in: t.frames_in,
        frames_out: t.frames_out,
        psnr_db: avg,
        min_psnr_db: min,
        cpu_secs: cpu,
        cpu_percent: if seconds > 0.0 {
            cpu / seconds * 100.0
        } else {
            0.0
        },
        rss_mb: after.1,
        wall_secs: wall,
        platform: super::select::current_platform(),
        gstreamer: super::gstreamer_version(),
        ok,
        note,
    })
}

fn build_video_pipeline(
    spec: &Spec,
    shared: &Arc<Shared>,
    frames: i32,
    width: i32,
    height: i32,
    fps: i32,
) -> Result<gst::Pipeline> {
    let pipeline = gst::Pipeline::with_name("codec-test");
    let src = make("videotestsrc", "src")?;
    src.set_property("num-buffers", frames);
    src.set_property_from_str("pattern", "smpte");
    let caps = crate::caps::CanvasCaps::video_at(width, height, gst::Fraction::new(fps, 1));
    let src_caps = capsfilter("src-caps", &caps)?;
    let overlay = make("timeoverlay", "motion").ok();
    let tee = make("tee", "t")?;
    tee.set_property("allow-not-linked", true);

    let ref_q = queue_thread("ref-q")?;
    let ref_sink = appsink("ref-sink");
    let enc_q = queue_thread("enc-q")?;
    let enc_conv = make("videoconvert", "enc-conv")?;
    let enc = make(&spec.encoder, "enc")?;
    let vars = Vars {
        fps: fps as i64,
        keyframe_frames: (fps * 2) as i64,
        ..Default::default()
    };
    apply::apply(&enc, &spec.properties, &vars);
    apply::apply_keyframe(&enc, spec.keyframe.as_ref(), &vars);

    let mut chain: Vec<gst::Element> = vec![enc_q.clone(), enc_conv.clone(), enc.clone()];
    if let Some(p) = &spec.parser {
        chain.push(make(p, "parse")?);
    }
    let Some(dec) = &spec.decoder else {
        bail!(
            "{} has no decoder, so there is nothing to compare against",
            spec.id
        );
    };
    chain.push(make(dec, "dec")?);
    if let Some(d) = &spec.download {
        if crate::probe::exists(d) {
            chain.push(make(d, "download")?);
        }
    }
    chain.push(make("videoconvert", "out-conv")?);
    chain.push(capsfilter("out-caps", &caps)?);
    let out_sink = appsink("out-sink");
    chain.push(out_sink.clone().upcast());

    let mut head: Vec<gst::Element> = vec![src.clone(), src_caps.clone()];
    if let Some(o) = &overlay {
        head.push(o.clone());
    }
    head.push(tee.clone());
    let refs: Vec<gst::Element> = vec![ref_q.clone(), ref_sink.clone().upcast()];

    let all: Vec<&gst::Element> = head.iter().chain(refs.iter()).chain(chain.iter()).collect();
    pipeline
        .add_many(all)
        .context("adding codec test elements")?;
    gst::Element::link_many(head.iter().collect::<Vec<_>>()).context("linking the source")?;
    gst::Element::link_many(std::iter::once(&tee).chain(refs.iter()).collect::<Vec<_>>())
        .context("linking the reference branch")?;
    gst::Element::link_many(
        std::iter::once(&tee)
            .chain(chain.iter())
            .collect::<Vec<_>>(),
    )
    .with_context(|| {
        format!(
            "linking {} through {}: this entry's elements do not agree on a format",
            spec.encoder, dec
        )
    })?;

    install_reference_callback(&ref_sink, shared.clone());
    install_output_callback(&out_sink, shared.clone());
    Ok(pipeline)
}

fn appsink(name: &str) -> gst_app::AppSink {
    gst_app::AppSink::builder()
        .name(name)
        .max_buffers(4)
        .drop(false)
        .sync(false)
        .build()
}

fn install_reference_callback(sink: &gst_app::AppSink, shared: Arc<Shared>) {
    sink.set_callbacks(
        gst_app::AppSinkCallbacks::builder()
            .new_sample(move |sink| {
                let sample = sink.pull_sample().map_err(|_| gst::FlowError::Eos)?;
                let Some(luma) = luma(&sample) else {
                    return Ok(gst::FlowSuccess::Ok);
                };
                let mut t = shared.tally.lock();
                while t.queue.len() >= LOOKAHEAD && !t.abort {
                    if shared.space.wait_for(&mut t, PATIENCE).timed_out() {
                        t.abort = true;
                    }
                }
                if t.abort {
                    return Err(gst::FlowError::Eos);
                }
                t.frames_in += 1;
                t.queue.push_back(luma);
                drop(t);
                shared.filled.notify_one();
                Ok(gst::FlowSuccess::Ok)
            })
            .eos(move |_| {})
            .build(),
    );
}

fn install_output_callback(sink: &gst_app::AppSink, shared: Arc<Shared>) {
    sink.set_callbacks(
        gst_app::AppSinkCallbacks::builder()
            .new_sample(move |sink| {
                let sample = sink.pull_sample().map_err(|_| gst::FlowError::Eos)?;
                let Some(got) = luma(&sample) else {
                    return Ok(gst::FlowSuccess::Ok);
                };
                let mut t = shared.tally.lock();
                while t.queue.is_empty() && !t.abort {
                    if shared.filled.wait_for(&mut t, PATIENCE).timed_out() {
                        t.abort = true;
                    }
                }
                if t.abort {
                    return Err(gst::FlowError::Eos);
                }
                let want = t.queue.pop_front();
                t.frames_out += 1;
                drop(t);
                shared.space.notify_one();
                if let Some(want) = want {
                    let mut t = shared.tally.lock();
                    if want.len() != got.len() {
                        t.mismatched += 1;
                    } else {
                        let db = psnr(&want, &got);
                        t.compared += 1;
                        t.psnr_sum += db;
                        t.psnr_min = t.psnr_min.min(db);
                    }
                }
                Ok(gst::FlowSuccess::Ok)
            })
            .build(),
    );
}

/// The luma plane, packed without its stride so two frames compare byte for
/// byte. Luma only: chroma errors show up in luma at these bitrates and one
/// plane keeps the comparison cheap enough to run inside the callback.
fn luma(sample: &gst::Sample) -> Option<Vec<u8>> {
    let caps = sample.caps()?;
    let info = gst_video::VideoInfo::from_caps(caps).ok()?;
    let buffer = sample.buffer()?;
    let frame = gst_video::VideoFrameRef::from_buffer_ref_readable(buffer, &info).ok()?;
    let w = info.width() as usize;
    let h = info.height() as usize;
    let stride = frame.plane_stride().first().copied().unwrap_or(0) as usize;
    let data = frame.plane_data(0).ok()?;
    if stride < w || data.len() < stride * h {
        return None;
    }
    let mut out = Vec::with_capacity(w * h);
    for row in 0..h {
        out.extend_from_slice(&data[row * stride..row * stride + w]);
    }
    Some(out)
}

pub fn psnr(a: &[u8], b: &[u8]) -> f64 {
    if a.is_empty() || a.len() != b.len() {
        return 0.0;
    }
    let mut sum = 0.0f64;
    for (x, y) in a.iter().zip(b.iter()) {
        let d = *x as f64 - *y as f64;
        sum += d * d;
    }
    let mse = sum / a.len() as f64;
    if mse <= 1e-12 {
        99.0
    } else {
        10.0 * (255.0 * 255.0 / mse).log10()
    }
}

/// Audio entries get the same round trip without the picture: encode, parse,
/// decode, and count what comes back. PSNR of a waveform is not a number
/// anyone acts on, so the check is that the samples flow and the durations
/// line up.
fn audio_round_trip(
    id: &str,
    codec: &str,
    encoder: &str,
    parser: Option<&str>,
    decoder: Option<&str>,
    properties: &BTreeMap<String, PropValue>,
    seconds: f64,
) -> Result<CodecReport> {
    let Some(decoder) = decoder else {
        bail!("{id} has no decoder to compare against");
    };
    let pipeline = gst::Pipeline::with_name("codec-test-audio");
    let src = make("audiotestsrc", "src")?;
    src.set_property("num-buffers", (seconds * 100.0) as i32);
    src.set_property_from_str("wave", "ticks");
    let src_caps = capsfilter(
        "src-caps",
        &gst::Caps::builder("audio/x-raw")
            .field("format", "S16LE")
            .field("rate", 48000i32)
            .field("channels", 2i32)
            .field("layout", "interleaved")
            .build(),
    )?;
    let conv = make("audioconvert", "conv")?;
    let enc = make(encoder, "enc")?;
    apply::apply(&enc, properties, &Vars::default());
    let mut chain = vec![src.clone(), src_caps.clone(), conv.clone(), enc.clone()];
    if let Some(p) = parser {
        chain.push(make(p, "parse")?);
    }
    chain.push(make(decoder, "dec")?);
    chain.push(make("audioconvert", "out-conv")?);
    let sink = appsink("out-sink");
    chain.push(sink.clone().upcast());
    pipeline
        .add_many(chain.iter().collect::<Vec<_>>())
        .context("adding audio test elements")?;
    gst::Element::link_many(chain.iter().collect::<Vec<_>>())
        .with_context(|| format!("linking {encoder} through {decoder}"))?;

    let count = Arc::new(std::sync::atomic::AtomicU64::new(0));
    {
        let count = count.clone();
        sink.set_callbacks(
            gst_app::AppSinkCallbacks::builder()
                .new_sample(move |sink| {
                    let _ = sink.pull_sample().map_err(|_| gst::FlowError::Eos)?;
                    count.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    Ok(gst::FlowSuccess::Ok)
                })
                .build(),
        );
    }
    let before = resources();
    let started = Instant::now();
    let outcome = run_to_eos(&pipeline, Duration::from_secs_f64(seconds * 3.0 + 20.0));
    let wall = started.elapsed().as_secs_f64();
    let _ = pipeline.set_state(gst::State::Null);
    let after = resources();
    let out = count.load(std::sync::atomic::Ordering::Relaxed);
    let cpu = (after.0 - before.0).max(0.0);
    let note = match &outcome {
        Ok(()) if out == 0 => "the pipeline ran but no audio came back out of the decoder".into(),
        Ok(()) => String::new(),
        Err(e) => format!("{e:#}"),
    };
    Ok(CodecReport {
        entry: id.into(),
        kind: "audio".into(),
        codec: codec.into(),
        accel: "software".into(),
        encoder: encoder.into(),
        decoder: Some(decoder.into()),
        width: 0,
        height: 0,
        fps: 0,
        seconds,
        frames_in: (seconds * 100.0) as u64,
        frames_out: out,
        psnr_db: None,
        min_psnr_db: None,
        cpu_secs: cpu,
        cpu_percent: if seconds > 0.0 {
            cpu / seconds * 100.0
        } else {
            0.0
        },
        rss_mb: after.1,
        wall_secs: wall,
        platform: super::select::current_platform(),
        gstreamer: super::gstreamer_version(),
        ok: note.is_empty(),
        note,
    })
}

/// Play until EOS or the first error, whichever comes first.
fn run_to_eos(pipeline: &gst::Pipeline, patience: Duration) -> Result<()> {
    pipeline
        .set_state(gst::State::Playing)
        .context("starting the test pipeline")?;
    let bus = pipeline.bus().context("the test pipeline has no bus")?;
    let deadline = Instant::now() + patience;
    loop {
        let left = deadline.saturating_duration_since(Instant::now());
        if left.is_zero() {
            bail!("the test pipeline produced neither an end of stream nor an error in time");
        }
        let Some(msg) = bus.timed_pop(gst::ClockTime::from_nseconds(left.as_nanos() as u64)) else {
            continue;
        };
        match msg.view() {
            gst::MessageView::Eos(_) => return Ok(()),
            gst::MessageView::Error(e) => {
                bail!(
                    "{}: {}",
                    e.src()
                        .map(|s| s.path_string().to_string())
                        .unwrap_or_else(|| "pipeline".into()),
                    e.error()
                );
            }
            _ => {}
        }
    }
}

/// CPU seconds used by this process so far, and its peak resident size in MB.
/// Process wide, not per pipeline, which is why the test runs one thing at a
/// time and the report says so.
#[cfg(unix)]
fn resources() -> (f64, f64) {
    unsafe {
        let mut u: libc::rusage = std::mem::zeroed();
        if libc::getrusage(libc::RUSAGE_SELF, &mut u) != 0 {
            return (0.0, 0.0);
        }
        let secs = |t: libc::timeval| t.tv_sec as f64 + t.tv_usec as f64 / 1_000_000.0;
        let cpu = secs(u.ru_utime) + secs(u.ru_stime);
        // ru_maxrss is bytes on macOS and kilobytes on Linux.
        let rss = if cfg!(target_os = "macos") {
            u.ru_maxrss as f64 / 1_048_576.0
        } else {
            u.ru_maxrss as f64 / 1024.0
        };
        (cpu, rss)
    }
}

#[cfg(not(unix))]
fn resources() -> (f64, f64) {
    // Windows has no getrusage. The round trip and the PSNR still run; the
    // cost columns read zero and the report says the platform does not give
    // them, which is better than a number nobody can trust.
    (0.0, 0.0)
}

/// One line per entry, each backed by a one second encode where the elements
/// exist. `gmx doctor` prints these.
pub fn doctor_lines(cat: &Catalogue, reg: &dyn Registry) -> Vec<String> {
    let mut out = Vec::new();
    for e in cat.video.iter().filter(|e| !e.disabled) {
        let id = e.id();
        let missing: Vec<String> = e
            .needs(Role::Encode)
            .into_iter()
            .filter(|n| !reg.has(n))
            .collect();
        if e.encoder.is_none() {
            out.push(decode_only_line(
                &id,
                e.decoder.as_deref().unwrap_or("?"),
                reg,
            ));
            continue;
        }
        if !missing.is_empty() {
            out.push(format!(
                "codec {id}: not installed here (missing {})",
                missing.join(", ")
            ));
            continue;
        }
        out.push(one_second(cat, &id));
    }
    for e in cat.audio.iter().filter(|e| !e.disabled) {
        let id = e.id();
        let missing: Vec<String> = e
            .needs(Role::Encode)
            .into_iter()
            .filter(|n| !reg.has(n))
            .collect();
        if e.encoder.is_none() {
            out.push(decode_only_line(
                &id,
                e.decoder.as_deref().unwrap_or("?"),
                reg,
            ));
            continue;
        }
        if !missing.is_empty() {
            out.push(format!(
                "codec {id}: not installed here (missing {})",
                missing.join(", ")
            ));
            continue;
        }
        out.push(one_second(cat, &id));
    }
    let platform = super::select::current_platform();
    for e in cat.graphics.iter().filter(|e| !e.disabled) {
        let id = e.id();
        let missing: Vec<String> = e.needs().into_iter().filter(|n| !reg.has(n)).collect();
        if !missing.is_empty() {
            out.push(format!(
                "graphics {id}: not installed here (missing {})",
                missing.join(", ")
            ));
        } else if e.memory == "system"
            || e.verified
                .iter()
                .any(|v| v.platform == platform || v.platform == "any")
        {
            out.push(format!(
                "graphics {id}: installed and verified for {platform}"
            ));
        } else {
            out.push(format!(
                "graphics {id}: installed but never tested on {platform}. \
                 Pin [hardware] graphics = \"{}\" and run the soak before a show depends on it.",
                e.accel
            ));
        }
    }
    out
}

fn decode_only_line(id: &str, decoder: &str, reg: &dyn Registry) -> String {
    if reg.has(decoder) {
        format!("codec {id}: decode only, {decoder} installed")
    } else {
        format!("codec {id}: decode only, {decoder} not installed here")
    }
}

fn one_second(cat: &Catalogue, id: &str) -> String {
    match test_entry(cat, id, 1.0, 640, 360, 30) {
        Ok(r) if r.ok => {
            let psnr = r
                .psnr_db
                .map(|p| format!(", psnr {p:.1} dB"))
                .unwrap_or_default();
            format!(
                "codec {id}: encodes here ({} frames in 1 s{}, {:.2} of one core at real time)",
                r.frames_out,
                psnr,
                r.cpu_percent / 100.0
            )
        }
        Ok(r) => format!(
            "codec {id}: elements load but the encode failed: {}",
            r.note
        ),
        Err(e) => format!("codec {id}: could not be tested: {e:#}"),
    }
}
