//! The send pipeline: the programme's sound, made MP3 or Ogg, to an Icecast
//! mount.
//!
//! ```text
//!   FIFO ─pump─► appsrc ─► matroskademux ─┬─► audio: decodebin ─► convert ─► resample ─► encoder ─► appsink ─► mount.rs
//!                                         └─► video: fakesink
//! ```
//!
//! This is the one output that has to encode: Icecast players want MP3 or
//! Ogg, and the programme's sound is AAC. An audio encode is a few percent of
//! one core. The picture is dropped at the demuxer and never decoded.
//!
//! The encoded bytes leave through an `appsink` to our own source client in
//! `mount.rs`, not `shout2send`, which GStreamer for Windows does not have.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use gmx_netkit::pipe::Pipe;
use godwinmix_capture_common::fifo::{Fifo, Pump};
use godwinmix_sdk::plugin::Reporter;
use gstreamer as gst;
use gstreamer::prelude::*;
use gstreamer_app as gst_app;

use crate::mount::{self, State};
use crate::settings::{Format, Settings};

pub const NEEDED: &[&str] = &["appsrc", "matroskademux", "decodebin", "appsink"];
/// Every format's encoder, listed so the trimmed runtime an installer carries
/// keeps them all; `start` checks only the one the settings pick.
// Read by dev/gst_trim.py, not by the code.
#[allow(dead_code)]
pub const ENCODERS_NEEDED: &[&str] = &["lamemp3enc", "mpegaudioparse", "vorbisenc", "opusenc", "oggmux"];

pub struct Sender {
    pub pipe: Pipe,
    pump: Option<Pump>,
    pub state: Arc<State>,
    stop: Arc<AtomicBool>,
    sending: Option<std::thread::JoinHandle<()>>,
}

/// The elements one format's encoder is made of.
fn encoder_elements(format: Format) -> &'static [&'static str] {
    match format {
        Format::Mp3 => &["lamemp3enc", "mpegaudioparse"],
        Format::OggVorbis => &["vorbisenc", "oggmux"],
        Format::OggOpus => &["opusenc", "oggmux"],
    }
}

/// The encoder for a format, as a launch fragment.
pub fn encoder(s: &Settings) -> String {
    match s.format {
        Format::Mp3 => format!("lamemp3enc target=bitrate cbr=true bitrate={} ! mpegaudioparse", s.bitrate_kbps),
        Format::OggVorbis => format!("vorbisenc bitrate={} ! oggmux", s.bitrate_kbps * 1000),
        Format::OggOpus => format!("audio/x-raw,rate=48000 ! opusenc bitrate={} ! oggmux", s.bitrate_kbps * 1000),
    }
}

/// The audio half: decode, encode, send, in one bin whose sink pad takes the
/// demuxer's audio. The decoder's pad is linked when it appears.
fn audio_bin(s: &Settings) -> Result<gst::Bin, String> {
    let desc = format!("audioconvert name=head ! audioresample ! {} ! appsink name=out sync=false async=false max-buffers=256 drop=true", encoder(s));
    let bin = gst::parse::bin_from_description(&desc, false).map_err(|e| format!("could not build the encoder: {e}"))?;
    let queue = gst::ElementFactory::make("queue").build().map_err(|e| e.to_string())?;
    let decode = gst::ElementFactory::make("decodebin").build().map_err(|e| e.to_string())?;
    bin.add_many([&queue, &decode]).map_err(|e| e.to_string())?;
    queue.link(&decode).map_err(|e| e.to_string())?;
    let head = bin.by_name("head").ok_or("no audioconvert")?.static_pad("sink").ok_or("no sink pad")?;
    decode.connect_pad_added(move |_, pad| {
        if !head.is_linked() {
            let _ = pad.link(&head);
        }
    });
    let ghost = gst::GhostPad::with_target(&queue.static_pad("sink").ok_or("no queue pad")?).map_err(|e| e.to_string())?;
    bin.add_pad(&ghost).map_err(|e| e.to_string())?;
    Ok(bin)
}

impl Sender {
    pub fn start(s: &Settings, fifo: Fifo, reporter: Option<Reporter>) -> Result<Sender, String> {
        gmx_netkit::init()?;
        gmx_netkit::elements::require(NEEDED)?;
        gmx_netkit::elements::require(encoder_elements(s.format))?;
        let pipeline = gst::Pipeline::with_name("gmx-icecast-output");
        let src = gst::ElementFactory::make("appsrc").name("in").build().map_err(|e| e.to_string())?;
        src.set_property("caps", gst::Caps::new_empty_simple("video/x-matroska"));
        src.set_property_from_str("format", "bytes");
        src.set_property("block", true);
        src.set_property("max-bytes", 4u64 * 1024 * 1024);
        let demux = gst::ElementFactory::make("matroskademux").build().map_err(|e| e.to_string())?;
        pipeline.add_many([&src, &demux]).map_err(|e| e.to_string())?;
        src.link(&demux).map_err(|e| e.to_string())?;
        let audio = audio_bin(s)?;
        pipeline.add(&audio).map_err(|e| e.to_string())?;
        let sink = audio.by_name("out").and_then(|o| o.downcast::<gst_app::AppSink>().ok()).ok_or("no appsink")?;
        route(&demux, &pipeline, audio);
        let (state, stop) = (Arc::new(State::default()), Arc::new(AtomicBool::new(false)));
        let sending = mount::spawn(s, sink, stop.clone(), state.clone());
        let mut pipe = Pipe::wrap(pipeline);
        pipe.play(reporter)?;
        Ok(Sender { pipe, pump: Some(Pump::start(fifo, src)), state, stop, sending: Some(sending) })
    }

    pub fn sent(&self) -> u64 {
        self.state.sent.load(Ordering::Relaxed)
    }

    /// How much programme has come in from the core so far.
    pub fn received(&self) -> u64 {
        self.pump.as_ref().map_or(0, Pump::bytes)
    }
}

impl Drop for Sender {
    fn drop(&mut self) {
        if let Some(mut p) = self.pump.take() {
            p.stop();
        }
        self.pipe.stop();
        self.stop.store(true, Ordering::Relaxed);
        if let Some(t) = self.sending.take() {
            let _ = t.join();
        }
    }
}

/// The first audio stream to the encoder; anything else to a fakesink.
fn route(demux: &gst::Element, pipeline: &gst::Pipeline, audio: gst::Bin) {
    let weak = pipeline.downgrade();
    demux.connect_pad_added(move |_, pad| {
        let Some(pipeline) = weak.upgrade() else { return };
        // By the pad's name: matroskademux may add it before its caps are set.
        let is_audio = pad.name().starts_with("audio");
        let sink = audio.static_pad("sink").filter(|p| is_audio && !p.is_linked());
        let target = match sink {
            Some(p) => p,
            None => {
                let Ok(fake) = gst::ElementFactory::make("fakesink").property("sync", false).build() else { return };
                let _ = pipeline.add(&fake);
                let _ = fake.sync_state_with_parent();
                let Some(p) = fake.static_pad("sink") else { return };
                p
            }
        };
        let _ = pad.link(&target);
    });
}
