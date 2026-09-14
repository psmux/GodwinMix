//! Audio monitoring: a branch on a raw audio tee, opened when a client asks
//! and removed when the last one goes.
//!
//! Meters are the cheap signal and are enough for most surfaces. This is for
//! the operator's headphones and for an agent that wants to run its own
//! silence or speech detection on the sound itself.
//!
//! # The branch
//!
//! ```text
//!  araw-tee -> queue (leaky, 1 s) -> audioconvert -> audioresample
//!           -> capsfilter (F32LE or S16LE, rate, channels) -> appsink
//! ```
//!
//! The queue is leaky for the same reason every preview queue is: the tee it
//! hangs off also carries the encoder, and a tee pushes to its pads one after
//! another on one thread, so a monitoring client that stops reading must not be
//! able to reach the programme. A late monitoring frame is worth nothing.
//!
//! The Opus branch is the same with `opusenc` before the sink.
//!
//! # Framing
//!
//! PCM goes out in fixed 10 ms frames with a 16 byte header: sequence as a
//! `u32`, four reserved bytes, then the running time in nanoseconds as a `u64`,
//! all little endian. The audio mixer does not hand out buffers of that size,
//! so the callback keeps a residue and cuts frames from it, deriving each
//! frame's running time from the first buffer's plus the samples emitted since.
//! That way the timestamps are continuous whatever the upstream buffer size is.
//!
//! Opus frames arrive one per 20 ms from the encoder and carry the same header.

use crate::caps::CanvasCaps;
use crate::gstutil::{self, make};
use anyhow::{Context, Result};
use gstreamer as gst;
use gstreamer::prelude::*;
use gstreamer_app as gst_app;
use parking_lot::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::sync::broadcast;
use tracing::{debug, warn};

/// Frames are dropped rather than queued for a client that cannot keep up.
/// Two is enough to cover a scheduling hiccup and short enough that nobody
/// listens to stale sound.
const CHANNEL_DEPTH: usize = 64;

/// The header in front of every frame: seq u32, reserved u32, running time u64,
/// little endian.
pub const HEADER_BYTES: usize = 16;

/// What a client asked for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AudioRequest {
    pub rate: i32,
    pub channels: i32,
    pub format: SampleFormat,
    pub codec: Codec,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SampleFormat {
    F32le,
    S16le,
}

impl SampleFormat {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "f32" | "f32le" | "F32LE" => Some(Self::F32le),
            "s16" | "s16le" | "S16LE" => Some(Self::S16le),
            _ => None,
        }
    }

    fn caps_name(self) -> &'static str {
        match self {
            Self::F32le => "F32LE",
            Self::S16le => "S16LE",
        }
    }

    fn bytes_per_sample(self) -> usize {
        match self {
            Self::F32le => 4,
            Self::S16le => 2,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Codec {
    /// Raw samples, cut into 10 ms frames here.
    Pcm,
    /// Opus at 48 kHz, 20 ms per frame.
    Opus,
}

impl Default for AudioRequest {
    fn default() -> Self {
        Self { rate: 48_000, channels: 2, format: SampleFormat::F32le, codec: Codec::Pcm }
    }
}

impl AudioRequest {
    /// Clamp to something a monitoring client can sensibly ask for. Opus is
    /// always 48 kHz whatever the query string says, because that is what the
    /// codec is.
    pub fn clamped(mut self) -> Self {
        self.rate = self.rate.clamp(8_000, 48_000);
        self.channels = self.channels.clamp(1, 2);
        if self.codec == Codec::Opus {
            self.rate = 48_000;
            self.format = SampleFormat::S16le;
        }
        self
    }

    /// Bytes in one 10 ms frame of raw audio at this shape.
    pub fn frame_bytes(&self) -> usize {
        (self.rate as usize / 100) * self.channels as usize * self.format.bytes_per_sample()
    }

    /// The kind this shows as in `gmx_stream_clients{kind}`.
    pub fn kind(&self) -> &'static str {
        match self.codec {
            Codec::Pcm => "pcm",
            Codec::Opus => "opus",
        }
    }

    /// The key a tap is shared under: everybody asking for the same shape of
    /// the same target shares one branch.
    pub fn key(&self, target: &str) -> String {
        format!(
            "{target}/{}/{}/{}/{}",
            self.kind(),
            self.rate,
            self.channels,
            self.format.caps_name()
        )
    }
}

/// One frame out: the header and the samples, already joined, because every
/// client wants them as one WebSocket binary frame.
pub type Frame = Arc<[u8]>;

/// A branch on a raw audio tee, and the channel its frames go out on.
///
/// Held by the mixer for as long as anything is listening. Dropping it takes
/// the branch off the tee and the elements to NULL.
pub struct AudioTap {
    key: String,
    tee: gst::Element,
    pad: Option<gst::Pad>,
    branch: Vec<gst::Element>,
    pipeline: gst::Pipeline,
    frames: broadcast::Sender<Frame>,
}

impl AudioTap {
    /// Build the branch and join it to `tee`, which must be a raw audio tee in
    /// `pipeline` carrying `allow-not-linked`.
    pub fn build(
        pipeline: &gst::Pipeline,
        tee: &gst::Element,
        key: &str,
        req: AudioRequest,
    ) -> Result<Self> {
        let req = req.clamped();
        let tag = sanitise(key);
        let (frames, _) = broadcast::channel(CHANNEL_DEPTH);

        let queue = gstutil::queue_preview(&format!("mon-q-{tag}"))?;
        let convert = make("audioconvert", &format!("mon-conv-{tag}"))?;
        let resample = make("audioresample", &format!("mon-res-{tag}"))?;
        let caps = gstutil::capsfilter(
            &format!("mon-caps-{tag}"),
            &CanvasCaps::audio_at(req.format.caps_name(), req.rate, req.channels),
        )?;

        let sink = gst_app::AppSink::builder()
            .name(format!("mon-sink-{tag}"))
            .max_buffers(8)
            .drop(true)
            .sync(false)
            .build();

        let mut branch: Vec<gst::Element> = vec![queue, convert, resample, caps];
        if req.codec == Codec::Opus {
            let enc = make("opusenc", &format!("mon-opus-{tag}"))
                .context("opusenc is not installed: /opus needs the GStreamer opus plugin")?;
            crate::probe::set_int(&enc, "bitrate", 64_000);
            crate::probe::set_int(&enc, "frame-size", 20);
            branch.push(enc);
        }
        let sink_el: gst::Element = sink.clone().upcast();
        branch.push(sink_el);

        install_callbacks(&sink, req, frames.clone());

        pipeline.add_many(&branch).context("adding an audio monitoring branch")?;
        gst::Element::link_many(branch.iter().collect::<Vec<_>>())
            .context("linking an audio monitoring branch")?;

        let mut tap = Self {
            key: key.to_string(),
            tee: tee.clone(),
            pad: None,
            branch,
            pipeline: pipeline.clone(),
            frames,
        };
        tap.attach()?;
        debug!(key, "audio monitoring branch built");
        Ok(tap)
    }

    fn attach(&mut self) -> Result<()> {
        let head = self.branch.first().context("an empty monitoring branch")?;
        let sink = head.static_pad("sink").context("the branch head has no sink pad")?;
        let pad = self
            .tee
            .request_pad_simple("src_%u")
            .context("the raw audio tee refused a pad for monitoring")?;
        pad.link(&sink).context("linking the monitoring branch onto the audio tee")?;
        for el in self.branch.iter().rev() {
            el.sync_state_with_parent().ok();
        }
        self.pad = Some(pad);
        Ok(())
    }

    /// A receiver for this tap's frames. Taking one does not keep the tap
    /// alive; the mixer's own count does that.
    pub fn subscribe(&self) -> broadcast::Receiver<Frame> {
        self.frames.subscribe()
    }

    pub fn key(&self) -> &str {
        &self.key
    }
}

impl Drop for AudioTap {
    /// Unlink, release the pad, then take the elements down and out. Nothing
    /// blocks: an element on its way to NULL is never handed another buffer,
    /// and the tee carries `allow-not-linked`.
    fn drop(&mut self) {
        if let Some(pad) = self.pad.take() {
            if let Some(peer) = pad.peer() {
                if let Err(e) = pad.unlink(&peer) {
                    warn!(key = %self.key, ?e, "could not unlink an audio monitoring branch");
                }
            }
            self.tee.release_request_pad(&pad);
        }
        for el in self.branch.iter().rev() {
            let _ = el.set_state(gst::State::Null);
            let _ = self.pipeline.remove(el);
        }
        debug!(key = %self.key, "audio monitoring branch removed");
    }
}

/// Cut the appsink's buffers into the frames a client expects and publish them.
///
/// Kept out of `build` so that both are short enough to read.
fn install_callbacks(
    sink: &gst_app::AppSink,
    req: AudioRequest,
    frames: broadcast::Sender<Frame>,
) {
    let seq = Arc::new(AtomicU64::new(0));
    // Residue between buffers, and the running time the next byte in it sits
    // at. Only the appsink's own thread touches this, but the callback is
    // `Fn`, so it lives behind a lock.
    let residue: Arc<Mutex<(Vec<u8>, Option<u64>)>> = Arc::new(Mutex::new((Vec::new(), None)));
    let frame_bytes = req.frame_bytes().max(1);
    let bytes_per_second =
        (req.rate as u64) * (req.channels as u64) * (req.format.bytes_per_sample() as u64);
    let passthrough = req.codec == Codec::Opus;

    sink.set_callbacks(
        gst_app::AppSinkCallbacks::builder()
            .new_sample(move |sink| {
                let sample = sink.pull_sample().map_err(|_| gst::FlowError::Eos)?;
                let buffer = sample.buffer().ok_or(gst::FlowError::Error)?;
                let pts = buffer.pts().map(|t| t.nseconds()).unwrap_or(0);
                let map = buffer.map_readable().map_err(|_| gst::FlowError::Error)?;

                if passthrough {
                    // Opus already arrives one frame per 20 ms.
                    let n = seq.fetch_add(1, Ordering::Relaxed) as u32;
                    let _ = frames.send(framed(n, pts, map.as_slice()));
                    return Ok(gst::FlowSuccess::Ok);
                }

                let mut held = residue.lock();
                if held.1.is_none() {
                    held.1 = Some(pts);
                }
                held.0.extend_from_slice(map.as_slice());
                while held.0.len() >= frame_bytes {
                    let chunk: Vec<u8> = held.0.drain(..frame_bytes).collect();
                    let at = held.1.unwrap_or(pts);
                    let n = seq.fetch_add(1, Ordering::Relaxed) as u32;
                    let _ = frames.send(framed(n, at, &chunk));
                    // The next frame starts exactly one frame later, so the
                    // running times are continuous whatever upstream did.
                    held.1 = Some(at + frame_bytes as u64 * 1_000_000_000 / bytes_per_second);
                }
                Ok(gst::FlowSuccess::Ok)
            })
            .build(),
    );
}

/// The 16 byte header and the payload, in one allocation.
fn framed(seq: u32, running_time_ns: u64, payload: &[u8]) -> Frame {
    let mut out = Vec::with_capacity(HEADER_BYTES + payload.len());
    out.extend_from_slice(&seq.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(&running_time_ns.to_le_bytes());
    out.extend_from_slice(payload);
    Arc::from(out)
}

/// Element names may not carry slashes.
fn sanitise(key: &str) -> String {
    key.chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '-' }).collect()
}

/// Read the three numbers out of a frame header.
pub fn parse_header(frame: &[u8]) -> Option<(u32, u64)> {
    if frame.len() < HEADER_BYTES {
        return None;
    }
    let seq = u32::from_le_bytes(frame[0..4].try_into().ok()?);
    let at = u64::from_le_bytes(frame[8..16].try_into().ok()?);
    Some((seq, at))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A mixer small enough to start inside a test.
    fn mixer_cfg() -> crate::config::Config {
        let mut cfg: crate::config::Config = toml::from_str("").unwrap();
        cfg.canvas = crate::config::Canvas {
            width: 320,
            height: 180,
            fps: 15,
            sample_rate: 48000,
            channels: 2,
        };
        cfg.multiview.enabled = false;
        cfg
    }

    /// How many monitoring branches this pipeline has right now. What proves
    /// the branch really went and not merely that a count was decremented.
    fn branches(pipeline: &gst::Pipeline) -> usize {
        pipeline
            .iterate_elements()
            .into_iter()
            .flatten()
            .filter(|e| e.name().starts_with("mon-sink-"))
            .count()
    }

    /// The acceptance test for audio monitoring: a client opens
    /// `/pcm/program`, gets frames of the right size with monotonic running
    /// times, closes, and the branch is gone.
    #[tokio::test(flavor = "multi_thread")]
    async fn a_pcm_client_gets_frames_and_leaving_removes_the_branch() {
        let _ = gst::init();
        let (mut mix, handle, cmd_rx, _bus_rx) = crate::mixer::Mixer::build(mixer_cfg()).unwrap();
        mix.start().unwrap();
        let preview = mix.preview_handle();
        let pipeline = mix.program_pipeline().clone();
        assert_eq!(branches(&pipeline), 0, "a monitoring branch before anybody asked");
        let thread = crate::mixer::spawn(mix, cmd_rx, handle.clone());

        let req = AudioRequest::default().clamped();
        let (stream, mut frames) = tokio::time::timeout(
            std::time::Duration::from_secs(5),
            preview.open_audio("program", req),
        )
        .await
        .expect("opening /pcm/program timed out")
        .unwrap_or_else(|e| panic!("the mixer refused /pcm/program: {e}"));
        assert_eq!(preview.clients().count("pcm"), 1);
        assert_eq!(branches(&pipeline), 1, "no branch was built");

        let mut last_seq = None;
        let mut last_at = None;
        for i in 0..10 {
            let frame = tokio::time::timeout(std::time::Duration::from_secs(5), frames.recv())
                .await
                .unwrap_or_else(|_| panic!("no frame {i} within five seconds"))
                .expect("the frame channel closed");
            assert_eq!(
                frame.len(),
                HEADER_BYTES + req.frame_bytes(),
                "frame {i} is not ten milliseconds of F32LE stereo plus a header"
            );
            let (seq, at) = parse_header(&frame).expect("a frame with no readable header");
            if let Some(prev) = last_seq {
                assert_eq!(seq, prev + 1, "the sequence skipped at frame {i}");
            }
            if let Some(prev) = last_at {
                assert!(at > prev, "running time went backwards at frame {i}: {prev} then {at}");
                assert_eq!(
                    at - prev,
                    10_000_000,
                    "frames must be ten milliseconds apart on the timeline"
                );
            }
            last_seq = Some(seq);
            last_at = Some(at);
        }

        drop(stream);
        for _ in 0..50 {
            if branches(&pipeline) == 0 {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
        assert_eq!(branches(&pipeline), 0, "the branch outlived its last client");
        assert_eq!(preview.clients().count("pcm"), 0);

        let _ = handle.send(crate::mixer::Command::Shutdown);
        tokio::task::spawn_blocking(move || thread.join()).await.unwrap().unwrap();
    }

    /// Two clients asking for the same shape share one branch.
    #[tokio::test(flavor = "multi_thread")]
    async fn two_clients_at_the_same_shape_share_one_branch() {
        let _ = gst::init();
        let (mut mix, handle, cmd_rx, _bus_rx) = crate::mixer::Mixer::build(mixer_cfg()).unwrap();
        mix.start().unwrap();
        let preview = mix.preview_handle();
        let pipeline = mix.program_pipeline().clone();
        let thread = crate::mixer::spawn(mix, cmd_rx, handle.clone());

        let open = |req| {
            let preview = preview.clone();
            async move {
                match preview.open_audio("program", req).await {
                    Ok(pair) => pair,
                    Err(e) => panic!("open_audio refused: {e}"),
                }
            }
        };
        let (a, _ra) = open(AudioRequest::default()).await;
        let (b, _rb) = open(AudioRequest::default()).await;
        assert_eq!(branches(&pipeline), 1, "two clients at one shape built two branches");
        assert_eq!(preview.clients().count("pcm"), 2);

        // A different shape is a second branch.
        let narrow = AudioRequest { channels: 1, rate: 16_000, ..Default::default() };
        let (c, _rc) = open(narrow).await;
        assert_eq!(branches(&pipeline), 2);

        drop(a);
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        assert_eq!(branches(&pipeline), 2, "a branch went while a client was still on it");
        drop(b);
        drop(c);
        for _ in 0..50 {
            if branches(&pipeline) == 0 {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
        assert_eq!(branches(&pipeline), 0);

        let _ = handle.send(crate::mixer::Command::Shutdown);
        tokio::task::spawn_blocking(move || thread.join()).await.unwrap().unwrap();
    }

    /// A target that is not here says what is, rather than a bare not found.
    #[tokio::test(flavor = "multi_thread")]
    async fn a_source_that_is_not_here_names_the_ones_that_are() {
        let _ = gst::init();
        let (mut mix, handle, cmd_rx, _bus_rx) = crate::mixer::Mixer::build(mixer_cfg()).unwrap();
        mix.start().unwrap();
        let preview = mix.preview_handle();
        let thread = crate::mixer::spawn(mix, cmd_rx, handle.clone());

        let e = match preview.open_audio("cam9", AudioRequest::default()).await {
            Ok(_) => panic!("a source that is not here must be refused"),
            Err(e) => e,
        };
        assert!(e.contains("cam9"), "{e}");
        assert!(e.contains("program"), "the refusal must name the next step: {e}");

        let _ = handle.send(crate::mixer::Command::Shutdown);
        tokio::task::spawn_blocking(move || thread.join()).await.unwrap().unwrap();
    }

    #[test]
    fn a_request_is_clamped_and_opus_is_always_48k() {
        let r = AudioRequest { rate: 96_000, channels: 7, ..Default::default() }.clamped();
        assert_eq!((r.rate, r.channels), (48_000, 2));
        let r = AudioRequest { rate: 8_000, codec: Codec::Opus, ..Default::default() }.clamped();
        assert_eq!(r.rate, 48_000, "opus is a 48 kHz codec whatever the query said");
    }

    #[test]
    fn ten_milliseconds_at_the_default_shape_is_the_size_the_docs_promise() {
        let r = AudioRequest::default().clamped();
        // 480 samples, two channels, four bytes each.
        assert_eq!(r.frame_bytes(), 3_840);
        let r = AudioRequest { format: SampleFormat::S16le, ..Default::default() }.clamped();
        assert_eq!(r.frame_bytes(), 1_920);
        let r = AudioRequest { channels: 1, rate: 16_000, ..Default::default() }.clamped();
        assert_eq!(r.frame_bytes(), 640);
    }

    #[test]
    fn a_header_says_what_it_is_and_reads_back() {
        let frame = framed(7, 1_234_567_890, &[1, 2, 3, 4]);
        assert_eq!(frame.len(), HEADER_BYTES + 4);
        assert_eq!(parse_header(&frame), Some((7, 1_234_567_890)));
        assert_eq!(&frame[HEADER_BYTES..], &[1, 2, 3, 4]);
        // Reserved stays zero so a later field can go there.
        assert_eq!(&frame[4..8], &[0, 0, 0, 0]);
        assert_eq!(parse_header(&[0u8; 4]), None);
    }

    #[test]
    fn a_format_reads_the_spellings_a_client_would_type() {
        assert_eq!(SampleFormat::parse("s16"), Some(SampleFormat::S16le));
        assert_eq!(SampleFormat::parse("f32"), Some(SampleFormat::F32le));
        assert_eq!(SampleFormat::parse("mp3"), None);
    }

    #[test]
    fn a_key_separates_shapes_so_two_clients_only_share_a_branch_when_they_match() {
        let a = AudioRequest::default().clamped();
        let b = AudioRequest { channels: 1, ..Default::default() }.clamped();
        assert_ne!(a.key("program"), b.key("program"));
        assert_eq!(a.key("program"), AudioRequest::default().clamped().key("program"));
        assert_ne!(a.key("program"), a.key("cam1"));
        assert!(sanitise("program/pcm/48000/2/F32LE").chars().all(|c| c != '/'));
    }
}
