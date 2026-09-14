//! Making an uploaded file safe to stream, and knowing when it already is.
//!
//! A file source plays today with no help from this module: the mixer opens it
//! with `uridecodebin` and normalises it to canvas caps like any other source.
//! What this adds is the operator's convenience: tell them whether a clip is
//! the kind every browser and player opens without argument (H.264 4:2:0 plus
//! AAC in an MP4), and if it is not, transcode a copy that is.
//!
//! The transcode is a GStreamer pipeline built in code, because the container
//! image ships the GStreamer elements but no ffmpeg binary. It runs on a
//! blocking thread, niced below the live programme, and one at a time, because
//! it competes with the programme encoder for the same cores.

use crate::probe;
use crate::state::Event;
use anyhow::{Context, Result};
use gstreamer as gst;
use gstreamer::prelude::*;
use gstreamer_pbutils::prelude::*;
use gstreamer_pbutils::{Discoverer, DiscovererInfo};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tracing::{info, warn};

use crate::gstutil::make;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConversionPhase {
    Running,
    Done,
    Failed,
}

/// One conversion, in flight or remembered after it finished.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ConversionState {
    pub state: ConversionPhase,
    /// 0.0 to 1.0, position over duration, both read off the pipeline.
    pub progress: f64,
    /// Set only on `failed`, shown to the operator verbatim: "no AAC encoder
    /// available" is worth more than "conversion failed".
    pub error: Option<String>,
    /// The `.web.mp4` name, on `done`.
    pub output: Option<String>,
}

/// The verdict on a file: whether it needs converting, and why if it does.
pub struct WebSafety {
    pub safe: bool,
    pub reasons: Vec<String>,
    pub video_codec: Option<String>,
    pub audio_codec: Option<String>,
    pub width: Option<u32>,
    pub height: Option<u32>,
}

/// The short codec name behind a stream's caps, e.g. "video/x-h264" -> "h264".
fn codec_of(caps: &gst::Caps) -> Option<String> {
    let name = caps.structure(0)?.name().to_string();
    Some(name.rsplit('/').next().unwrap_or(&name).trim_start_matches("x-").to_string())
}

/// Decide whether a file is what a browser opens without transcoding.
///
/// The bar is compatibility, not quality: a poor looking H.264 file converts to
/// one that looks exactly as poor, so re-encoding it buys nothing. H.264 in an
/// MP4 or MOV, AAC or no audio, even dimensions.
pub fn web_safety(info: &DiscovererInfo) -> WebSafety {
    let mut reasons = Vec::new();

    let container = info
        .stream_info()
        .and_then(|s| s.caps())
        .and_then(|c| c.structure(0).map(|s| s.name().to_string()))
        .unwrap_or_default();
    // quicktime covers both mp4 and mov. Anything else (Matroska, WebM, AVI,
    // FLV, MPEG-TS, WMV) is repackaged as well as recoded.
    if !container.contains("quicktime") && !container.is_empty() {
        reasons.push(format!("container is {container}, not mp4"));
    }

    let vstreams = info.video_streams();
    let (video_codec, width, height) = match vstreams.first() {
        Some(v) => {
            let caps = v.caps();
            let codec = caps.as_ref().and_then(codec_of);
            if codec.as_deref() != Some("h264") {
                reasons.push(format!(
                    "video is {}, not H.264",
                    codec.clone().unwrap_or_else(|| "unknown".into())
                ));
            }
            let (w, h) = (v.width(), v.height());
            if w % 2 != 0 || h % 2 != 0 {
                reasons.push(format!("odd dimensions {w}x{h}"));
            }
            (codec, (w != 0).then_some(w), (h != 0).then_some(h))
        }
        None => {
            reasons.push("no video track".into());
            (None, None, None)
        }
    };
    if vstreams.len() > 1 {
        reasons.push(format!("{} video tracks", vstreams.len()));
    }

    let astreams = info.audio_streams();
    let audio_codec = astreams.first().and_then(|a| a.caps()).as_ref().and_then(codec_of);
    if let Some(codec) = &audio_codec {
        // AAC's caps name is "audio/mpeg"; anything else (mp3, opus, vorbis,
        // ac3, raw) is recoded.
        if codec != "mpeg" && codec != "aac" {
            reasons.push(format!("audio is {codec}, not AAC"));
        }
    }
    if astreams.len() > 1 {
        reasons.push(format!("{} audio tracks", astreams.len()));
    }

    WebSafety {
        safe: reasons.is_empty(),
        reasons,
        // Report the codec as "aac" rather than the caps spelling "mpeg".
        video_codec,
        audio_codec: audio_codec.map(|c| if c == "mpeg" { "aac".into() } else { c }),
        width,
        height,
    }
}

/// Whether the moov atom comes before the mdat, which is what lets a player
/// start before the whole file has arrived. Only matters over HTTP; the mixer
/// opens its own copy and seeks freely. None for a non-ISO file.
pub fn moov_first(path: &Path) -> Option<bool> {
    use std::io::{Read, Seek, SeekFrom};
    let mut f = std::fs::File::open(path).ok()?;
    let len = f.metadata().ok()?.len();
    let mut pos: u64 = 0;
    let mut saw_iso = false;
    while pos + 8 <= len {
        f.seek(SeekFrom::Start(pos)).ok()?;
        let mut hdr = [0u8; 8];
        f.read_exact(&mut hdr).ok()?;
        let size32 = u32::from_be_bytes([hdr[0], hdr[1], hdr[2], hdr[3]]) as u64;
        let kind = std::str::from_utf8(&hdr[4..8]).ok()?.to_string();
        let (box_len, header) = match size32 {
            1 => {
                let mut ext = [0u8; 8];
                f.read_exact(&mut ext).ok()?;
                (u64::from_be_bytes(ext), 16u64)
            }
            0 => (len - pos, 8),
            n => (n, 8),
        };
        match kind.as_str() {
            "ftyp" => saw_iso = true,
            "moov" => return Some(true),
            "mdat" => return Some(saw_iso.then_some(false)?),
            _ => {}
        }
        if box_len < header {
            return None;
        }
        pos += box_len;
    }
    None
}

/// The converted sibling of a source file: `clip.mkv` -> `clip.web.mp4`.
pub fn converted_sibling(path: &Path) -> PathBuf {
    let stem = path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    path.with_file_name(format!("{stem}.web.mp4"))
}

/// True for a name this module produced, so the library can fold it onto its
/// original rather than list it twice.
pub fn is_converted_name(name: &str) -> bool {
    name.ends_with(".web.mp4")
}

pub struct Converter {
    jobs: Mutex<HashMap<String, ConversionState>>,
    /// One conversion at a time: a transcode competes with the live programme
    /// encoder for cores. A second request queues on this permit.
    slot: Arc<tokio::sync::Semaphore>,
    events: crate::mixer::MixerHandle,
    threads: u32,
    probe_timeout_secs: u64,
}

impl Converter {
    pub fn new(events: crate::mixer::MixerHandle, threads: u32, probe_timeout_secs: u64) -> Self {
        Self {
            jobs: Mutex::new(HashMap::new()),
            slot: Arc::new(tokio::sync::Semaphore::new(1)),
            events,
            threads: threads.max(1),
            probe_timeout_secs,
        }
    }

    pub fn state(&self, name: &str) -> Option<ConversionState> {
        self.jobs.lock().get(name).cloned()
    }

    /// Start converting `name` (a plain file name in the media directory).
    /// Returns as soon as the job is accepted: a clip is tens of seconds and
    /// the operator has a show on.
    pub fn start(self: &Arc<Self>, name: String, input: PathBuf) -> Result<ConversionState> {
        {
            let mut jobs = self.jobs.lock();
            if matches!(jobs.get(&name), Some(s) if s.state == ConversionPhase::Running) {
                anyhow::bail!("{name} is already converting");
            }
            jobs.insert(
                name.clone(),
                ConversionState { state: ConversionPhase::Running, progress: 0.0, error: None, output: None },
            );
        }
        self.emit(&name);

        let accepted = self.jobs.lock().get(&name).cloned().unwrap();
        let me = Arc::clone(self);
        tokio::task::spawn_blocking(move || {
            // A blocking GStreamer state change must never sit on a Tokio
            // worker; this whole job runs here.
            let permit = me.slot.clone().try_acquire_owned();
            let _permit = match permit {
                Ok(p) => p,
                Err(_) => {
                    // Wait for the running job by blocking on the async permit
                    // from this blocking thread.
                    match tokio::runtime::Handle::current().block_on(me.slot.clone().acquire_owned()) {
                        Ok(p) => p,
                        Err(e) => {
                            me.finish(&name, Err(anyhow::anyhow!("converter closed: {e}")));
                            return;
                        }
                    }
                }
            };
            let out = converted_sibling(&input);
            let result = me.run(&name, &input, &out);
            me.finish(&name, result.map(|_| out));
        });

        Ok(accepted)
    }

    fn emit(&self, name: &str) {
        let conversion = self.jobs.lock().get(name).cloned();
        self.events.emit(Event::MediaChanged { name: name.to_string(), conversion });
    }

    fn set_progress(&self, name: &str, progress: f64) {
        if let Some(s) = self.jobs.lock().get_mut(name) {
            s.progress = progress;
        }
        self.emit(name);
    }

    fn finish(&self, name: &str, result: Result<PathBuf>) {
        {
            let mut jobs = self.jobs.lock();
            let entry = jobs.entry(name.to_string()).or_insert(ConversionState {
                state: ConversionPhase::Running,
                progress: 0.0,
                error: None,
                output: None,
            });
            match &result {
                Ok(out) => {
                    entry.state = ConversionPhase::Done;
                    entry.progress = 1.0;
                    entry.output = out.file_name().map(|n| n.to_string_lossy().into_owned());
                }
                Err(e) => {
                    entry.state = ConversionPhase::Failed;
                    entry.error = Some(format!("{e:#}"));
                }
            }
        }
        match &result {
            Ok(out) => info!(%name, output = %out.display(), "conversion done"),
            Err(e) => {
                warn!(%name, error = %format!("{e:#}"), "conversion failed");
                self.events.emit(Event::Alert {
                    severity: crate::state::Severity::Error,
                    message: format!("Converting {name} failed: {e:#}"),
                });
            }
        }
        self.emit(name);
    }

    /// Build and run the transcode pipeline, output written to `<out>.part`
    /// and renamed on success so a half-written file never appears in the
    /// listing.
    fn run(&self, name: &str, input: &Path, out: &Path) -> Result<()> {
        let info = Discoverer::new(gst::ClockTime::from_seconds(self.probe_timeout_secs.max(1)))
            .context("creating discoverer")?
            .discover_uri(&crate::input::to_uri(&input.display().to_string()))
            .context("inspecting the file to convert")?;
        let has_audio = !info.audio_streams().is_empty();
        let duration_ns = info.duration().map(|d| d.nseconds()).unwrap_or(0);

        let safety = web_safety(&info);
        let odd = safety.width.map(|w| w % 2 != 0).unwrap_or(false)
            || safety.height.map(|h| h % 2 != 0).unwrap_or(false);

        let aenc_factory = probe::best_audio_encoder()
            .context("no AAC encoder available to convert with")?;

        // Lower this thread's priority before the pipeline (and x264's worker
        // threads, created during the state change) exist. Per-thread nice is
        // inherited, so one call covers the encode. The programme encoder,
        // started long ago on another thread, keeps its priority.
        nice_this_thread(10);

        let part = out.with_extension("mp4.part");
        let pipeline = gst::Pipeline::new();

        let src = make("filesrc", "conv-src")?;
        src.set_property("location", input.to_string_lossy().as_ref());
        let dec = make("decodebin", "conv-dec")?;
        let mux = make("mp4mux", "conv-mux")?;
        mux.set_property("faststart", true);
        let sink = make("filesink", "conv-sink")?;
        sink.set_property("location", part.to_string_lossy().as_ref());

        pipeline.add_many([&src, &dec, &mux, &sink]).context("adding converter elements")?;
        src.link(&dec).context("linking filesrc to decodebin")?;
        mux.link(&sink).context("linking mux to filesink")?;

        // Video branch: convert, optionally scale to even, encode H.264.
        let vq = make("queue", "conv-vq")?;
        let vconv = make("videoconvert", "conv-vconv")?;
        let vscale = make("videoscale", "conv-vscale")?;
        let vcaps = crate::gstutil::capsfilter(
            "conv-vcaps",
            &gst::Caps::builder("video/x-raw").field("format", "I420").build(),
        )?;
        let venc = make("x264enc", "conv-venc")?;
        // A file transcode, not a live output: a quality target keeps a static
        // graphic sharp, a slower preset is affordable when nothing waits on
        // the frame, and the thread cap stops it starving the programme.
        probe::set_enum(&venc, "pass", "quant");
        probe::set_int(&venc, "quantizer", 21);
        probe::set_enum(&venc, "speed-preset", "medium");
        probe::set_int(&venc, "bframes", 0);
        probe::set_bool(&venc, "byte-stream", false);
        probe::set_int(&venc, "threads", self.threads as i64);
        let vprofile = crate::gstutil::capsfilter(
            "conv-vprofile",
            &gst::Caps::builder("video/x-h264").field("profile", "main").build(),
        )?;
        let vparse = make("h264parse", "conv-vparse")?;
        pipeline
            .add_many([&vq, &vconv, &vscale, &vcaps, &venc, &vprofile, &vparse])
            .context("adding video branch")?;
        gst::Element::link_many([&vq, &vconv, &vscale, &vcaps, &venc, &vprofile, &vparse])
            .context("linking video branch")?;
        vparse.link_pads(Some("src"), &mux, Some("video_%u")).context("video to mux")?;
        if odd {
            info!("converting odd dimensions; videoscale will round to even");
        }

        // Audio branch, only when the file has audio.
        let audio_sink_pad = if has_audio {
            let aq = make("queue", "conv-aq")?;
            let aconv = make("audioconvert", "conv-aconv")?;
            let ares = make("audioresample", "conv-ares")?;
            let acaps = crate::gstutil::capsfilter(
                "conv-acaps",
                &gst::Caps::builder("audio/x-raw").field("rate", 48000i32).field("channels", 2i32).build(),
            )?;
            let aenc = make(aenc_factory, "conv-aenc")?;
            probe::configure_audio_encoder(&aenc, 160);
            let aparse = make("aacparse", "conv-aparse")?;
            pipeline
                .add_many([&aq, &aconv, &ares, &acaps, &aenc, &aparse])
                .context("adding audio branch")?;
            gst::Element::link_many([&aq, &aconv, &ares, &acaps, &aenc, &aparse])
                .context("linking audio branch")?;
            aparse.link_pads(Some("src"), &mux, Some("audio_%u")).context("audio to mux")?;
            Some(aq.static_pad("sink").context("audio queue has no sink pad")?)
        } else {
            None
        };

        // Route decodebin's dynamic pads to the branches. The video sink is
        // taken once; a file with several video tracks sends the rest nowhere,
        // which is what web_safety warned about.
        let vq_sink = vq.static_pad("sink").context("video queue has no sink pad")?;
        let vq_taken = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        dec.connect_pad_added(move |_el, pad| {
            let media = pad
                .current_caps()
                .and_then(|c| c.structure(0).map(|s| s.name().to_string()))
                .unwrap_or_default();
            let target = if media.starts_with("video/") {
                if vq_taken.swap(true, std::sync::atomic::Ordering::SeqCst) {
                    return;
                }
                Some(&vq_sink)
            } else if media.starts_with("audio/") {
                audio_sink_pad.as_ref()
            } else {
                None
            };
            if let Some(sink) = target {
                if sink.is_linked() {
                    return;
                }
                if let Err(e) = pad.link(sink) {
                    warn!(%media, ?e, "converter could not link a decoded pad");
                }
            }
        });

        pipeline.set_state(gst::State::Playing).context("starting the converter")?;
        let bus = pipeline.bus().context("converter pipeline has no bus")?;

        let started = std::time::Instant::now();
        let deadline = std::time::Duration::from_secs(30 * 60);
        let result = loop {
            if started.elapsed() > deadline {
                break Err(anyhow::anyhow!("conversion timed out after 30 minutes"));
            }
            let Some(msg) = bus.timed_pop(gst::ClockTime::from_mseconds(200)) else {
                if duration_ns > 0 {
                    if let Some(pos) = pipeline.query_position::<gst::ClockTime>() {
                        let p = (pos.nseconds() as f64 / duration_ns as f64).clamp(0.0, 0.999);
                        self.set_progress(name, p);
                    }
                }
                continue;
            };
            use gst::MessageView;
            match msg.view() {
                MessageView::Eos(_) => break Ok(()),
                MessageView::Error(e) => {
                    break Err(anyhow::anyhow!(
                        "{}: {}",
                        e.error(),
                        e.debug().unwrap_or_default()
                    ))
                }
                _ => {}
            }
        };
        let _ = pipeline.set_state(gst::State::Null);

        result?;
        // fsync before the rename: a power cut between the two leaves a named
        // file that is not all there, and the next take would play it.
        if let Ok(f) = std::fs::File::open(&part) {
            let _ = f.sync_all();
        }
        std::fs::rename(&part, out).with_context(|| format!("renaming {} to {}", part.display(), out.display()))?;
        Ok(())
    }
}

/// Drop this thread's priority. Per-thread on Linux and inherited by threads
/// created from it, so x264's workers, created during the state change made
/// from here, are covered too.
#[cfg(target_os = "linux")]
fn nice_this_thread(delta: i32) {
    unsafe {
        libc::setpriority(libc::PRIO_PROCESS, 0, delta);
    }
}
#[cfg(not(target_os = "linux"))]
fn nice_this_thread(_delta: i32) {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_element_the_converter_needs_is_installed() {
        let _ = gst::init();
        for f in ["filesrc", "decodebin", "videoconvert", "videoscale", "x264enc", "h264parse", "aacparse", "mp4mux", "filesink"] {
            assert!(probe::exists(f), "missing GStreamer element: {f}");
        }
        assert!(probe::best_audio_encoder().is_some(), "no AAC encoder installed");
    }

    #[test]
    fn moov_first_reads_the_box_order() {
        // moov before mdat.
        let dir = std::env::temp_dir().join(format!("gmx-moov-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let mut good = Vec::new();
        good.extend_from_slice(&16u32.to_be_bytes());
        good.extend_from_slice(b"ftyp");
        good.extend_from_slice(b"isom\0\0\0\0");
        good.extend_from_slice(&9u32.to_be_bytes());
        good.extend_from_slice(b"moov");
        good.push(0);
        let gp = dir.join("good.mp4");
        std::fs::write(&gp, &good).unwrap();
        assert_eq!(moov_first(&gp), Some(true));

        // mdat before moov.
        let mut bad = Vec::new();
        bad.extend_from_slice(&16u32.to_be_bytes());
        bad.extend_from_slice(b"ftyp");
        bad.extend_from_slice(b"isom\0\0\0\0");
        bad.extend_from_slice(&9u32.to_be_bytes());
        bad.extend_from_slice(b"mdat");
        bad.push(0);
        let bp = dir.join("bad.mp4");
        std::fs::write(&bp, &bad).unwrap();
        assert_eq!(moov_first(&bp), Some(false));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn converted_sibling_is_web_mp4_beside_the_original() {
        assert_eq!(converted_sibling(Path::new("/m/clip.mkv")), Path::new("/m/clip.web.mp4"));
        assert!(is_converted_name("clip.web.mp4"));
        assert!(!is_converted_name("clip.mp4"));
    }
}
