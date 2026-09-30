//! The send pipeline: the programme's sound, made MP3 or Ogg, to an Icecast
//! mount.
//!
//! ```text
//!   FIFO ─pump─► appsrc ─► matroskademux ─┬─► audio: decodebin ─► convert ─► resample ─► encoder ─► shout2send
//!                                         └─► video: fakesink
//! ```
//!
//! This is the one output that has to encode: Icecast players want MP3 or
//! Ogg, and the programme's sound is AAC. An audio encode is a few percent of
//! one core. The picture is dropped at the demuxer and never decoded.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use gmx_netkit::pipe::Pipe;
use godwinmix_capture_common::fifo::{Fifo, Pump};
use godwinmix_sdk::plugin::Reporter;
use gstreamer as gst;
use gstreamer::prelude::*;

use crate::settings::{Format, Settings};

pub const NEEDED: &[&str] = &["appsrc", "matroskademux", "decodebin", "shout2send"];

pub struct Sender {
    pub pipe: Pipe,
    pump: Option<Pump>,
    pub bytes: Arc<AtomicU64>,
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
    let desc = format!("audioconvert name=head ! audioresample ! {} ! shout2send name=out sync=false async=false", encoder(s));
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
    let out = bin.by_name("out").ok_or("no shout2send")?;
    out.set_property("ip", &s.host);
    out.set_property("port", i32::from(s.port));
    out.set_property("mount", &s.mount);
    out.set_property("username", &s.user);
    out.set_property("password", &s.password);
    out.set_property("streamname", &s.name);
    out.set_property("public", s.public);
    out.set_property_from_str("protocol", "http");
    // The programme's tags are not song titles; a title update is also an
    // admin request most source logins may not make.
    out.set_property("send-title-info", false);
    Ok(bin)
}

impl Sender {
    pub fn start(s: &Settings, fifo: Fifo, reporter: Option<Reporter>) -> Result<Sender, String> {
        gmx_netkit::init()?;
        gmx_netkit::elements::require(NEEDED)?;
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
        let bytes = Arc::new(AtomicU64::new(0));
        count_into(&audio, &bytes);
        route(&demux, &pipeline, audio);
        let mut pipe = Pipe::wrap(pipeline);
        pipe.play(reporter)?;
        Ok(Sender { pipe, pump: Some(Pump::start(fifo, src)), bytes })
    }
}

impl Drop for Sender {
    fn drop(&mut self) {
        if let Some(mut p) = self.pump.take() {
            p.stop();
        }
        self.pipe.stop();
    }
}

/// Count what reaches the sink, for health.
fn count_into(audio: &gst::Bin, bytes: &Arc<AtomicU64>) {
    let Some(pad) = audio.by_name("out").and_then(|o| o.static_pad("sink")) else { return };
    let b = bytes.clone();
    pad.add_probe(gst::PadProbeType::BUFFER, move |_, info| {
        if let Some(buf) = info.buffer() {
            b.fetch_add(buf.size() as u64, Ordering::Relaxed);
        }
        gst::PadProbeReturn::Ok
    });
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
