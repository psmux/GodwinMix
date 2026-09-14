//! The two pipelines: taking NDI in, and putting NDI out.
//!
//! ```text
//!   source   ndisrc ──► ndisrcdemux ──┬─► queue ─► matroskamux ─► fdsink fd=1
//!                                     └─► queue ─┘
//!
//!   output   filesrc(the core's FIFO) ─► matroskademux ─┬─► decodebin ─► ndisinkcombiner ─► ndisink
//!                                                       └─► decodebin ─┘
//! ```
//!
//! NDI's own codec is SpeedHQ and only the runtime can decode it, so `ndisrc`
//! hands over raw frames and that is what crosses to the core: raw video in a
//! streamable Matroska stream on a pipe. The media contract in the plugin
//! architecture prices that at 41 MB/s for 720p30 and 93 MB/s for 1080p30, paid
//! in one memcpy per frame and no encode and no decode. An `unixfd` transport
//! would remove even the copy; it is the obvious next step and is not here
//! because it cannot be tested on a machine with no NDI runtime, and shipping
//! an untested transport is worse than shipping one copy per frame.
//!
//! The output pays a decode: the core hands an output the encoded programme,
//! and `ndisink` wants raw. That is the protocol's price, not this plugin's.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use gmx_netkit::pipe::Pipe;
use godwinmix_sdk::plugin::Reporter;
use godwinmix_sdk::wire::Health;
use gstreamer as gst;
use gstreamer::prelude::*;

/// What the source provide needs from GStreamer.
pub const SOURCE_NEEDED: &[&str] = &["ndisrc", "ndisrcdemux", "matroskamux"];
/// What the output provide needs.
pub const OUTPUT_NEEDED: &[&str] = &["ndisink", "ndisinkcombiner", "matroskademux"];

const POLL_MS: u64 = 1000;

/// The settings of `ndi/source`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SourceSettings {
    /// The NDI name, as `list_senders` reports it: `STUDIO (CAM 1)`.
    pub name: String,
    /// `host:port`, for a sender mDNS cannot reach (another subnet, a VPN).
    pub address: String,
    /// `low` halves the bandwidth and the resolution; `high` is the full one.
    pub bandwidth: String,
    /// Timestamp mode, passed through to `ndisrc`.
    pub timestamp_mode: String,
}

impl SourceSettings {
    pub fn from_params(params: &serde_json::Value) -> SourceSettings {
        let string = |key: &str| {
            params
                .get(key)
                .and_then(|v| v.as_str())
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
        };
        SourceSettings {
            name: string("name").unwrap_or_default(),
            address: string("address").unwrap_or_default(),
            bandwidth: string("bandwidth").unwrap_or_else(|| "high".into()),
            timestamp_mode: string("timestamp_mode").unwrap_or_else(|| "receive-time-vs-timecode".into()),
        }
    }

    /// What is wrong with these settings. A missing name is not: `configure`
    /// before `start` is legal, and that is how the name often arrives.
    pub fn malformed(&self) -> Option<String> {
        if !matches!(self.bandwidth.as_str(), "high" | "low" | "audio-only") {
            return Some(format!(
                "bandwidth is '{}'. It is 'high' (the full picture), 'low' (a smaller \
                 one at a fraction of the bandwidth, for a preview or a weak network) \
                 or 'audio-only'.",
                self.bandwidth
            ));
        }
        None
    }

    /// What stops it connecting now.
    pub fn problem(&self) -> Option<String> {
        if self.name.is_empty() && self.address.is_empty() {
            return Some(
                "there is no sender. Set 'name' to the NDI name of the sender, which the \
                 list_senders tool reports, or 'address' to its host:port when mDNS \
                 cannot reach it."
                    .into(),
            );
        }
        self.malformed()
    }

    /// How this source names itself in a log line.
    pub fn describe(&self) -> String {
        if self.name.is_empty() {
            format!("the NDI sender at {}", self.address)
        } else {
            format!("the NDI sender '{}'", self.name)
        }
    }
}

/// A running NDI receiver.
pub struct Receiver {
    pipe: Pipe,
    settings: SourceSettings,
    stop: Arc<AtomicBool>,
    poller: Option<std::thread::JoinHandle<()>>,
}

impl Receiver {
    pub fn start(
        settings: &SourceSettings,
        reporter: Option<Reporter>,
        file: Option<std::path::PathBuf>,
    ) -> Result<Receiver, String> {
        gmx_netkit::init()?;
        gmx_netkit::elements::require(SOURCE_NEEDED)?;

        let pipeline = gst::Pipeline::with_name("gmx-ndi-source");
        let src = make("ndisrc", "ndi")?;
        set_string(&src, "ndi-name", &settings.name);
        set_string(&src, "url-address", &settings.address);
        set_enum(&src, "bandwidth", &settings.bandwidth);
        set_enum(&src, "timestamp-mode", &settings.timestamp_mode);

        let demux = make("ndisrcdemux", "demux")?;
        let mux = make("matroskamux", "mux")?;
        mux.set_property("streamable", true);
        let sink = match &file {
            None => {
                let out = make("fdsink", "out")?;
                out.set_property("fd", 1i32);
                out
            }
            Some(path) => {
                let out = make("filesink", "out")?;
                out.set_property("location", path.to_string_lossy().to_string());
                out
            }
        };
        for name in ["sync", "async"] {
            if sink.find_property(name).is_some() {
                sink.set_property(name, false);
            }
        }

        pipeline
            .add_many([&src, &demux, &mux, &sink])
            .map_err(|e| format!("could not assemble the NDI pipeline: {e}"))?;
        gst::Element::link(&src, &demux)
            .map_err(|e| format!("could not link ndisrc to its demuxer: {e}"))?;
        gst::Element::link(&mux, &sink)
            .map_err(|e| format!("could not link the muxer to the pipe: {e}"))?;

        let weak = pipeline.downgrade();
        let for_pads = reporter.clone();
        demux.connect_pad_added(move |_, pad| {
            let Some(pipeline) = weak.upgrade() else { return };
            if let Err(e) = attach(&pipeline, pad, "mux") {
                if let Some(r) = &for_pads {
                    r.error(format!("an NDI stream could not be muxed: {e}"));
                }
            }
        });

        let mut pipe = Pipe::wrap(pipeline);
        pipe.play(reporter.clone()).map_err(|e| {
            format!(
                "{e}. {} did not start. Check the sender is running and that \
                 list_senders can see it.",
                settings.describe()
            )
        })?;
        let stop = Arc::new(AtomicBool::new(false));
        let poller = spawn_poll(
            pipe.watch(),
            settings.describe(),
            reporter,
            Arc::clone(&stop),
        );
        Ok(Receiver { pipe, settings: settings.clone(), stop, poller })
    }

    pub fn health(&self) -> Health {
        health_of(&self.pipe.watch(), &self.settings.describe())
    }
}

impl Drop for Receiver {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(thread) = self.poller.take() {
            let _ = thread.join();
        }
        self.pipe.stop();
    }
}

/// The settings of `ndi/output`.
#[derive(Debug, Clone, PartialEq)]
pub struct OutputSettings {
    /// The name this mixer announces itself under on the network.
    pub name: String,
}

impl Default for OutputSettings {
    fn default() -> OutputSettings {
        OutputSettings { name: "GodwinMix".into() }
    }
}

impl OutputSettings {
    pub fn from_params(params: &serde_json::Value) -> OutputSettings {
        OutputSettings {
            name: params
                .get("name")
                .and_then(|v| v.as_str())
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| OutputSettings::default().name),
        }
    }
}

/// A running NDI sender: the programme announced on the network.
pub struct Announcer {
    pipe: Pipe,
    settings: OutputSettings,
    stop: Arc<AtomicBool>,
    poller: Option<std::thread::JoinHandle<()>>,
}

impl Announcer {
    pub fn start(
        settings: &OutputSettings,
        fifo: &str,
        reporter: Option<Reporter>,
    ) -> Result<Announcer, String> {
        gmx_netkit::init()?;
        gmx_netkit::elements::require(OUTPUT_NEEDED)?;
        if fifo.trim().is_empty() {
            return Err(
                "start.params.media is empty. An output reads the encoded programme from \
                 the FIFO the core names there; without it there is nothing to send."
                    .into(),
            );
        }

        let pipeline = gst::Pipeline::with_name("gmx-ndi-output");
        let src = make("filesrc", "fifo")?;
        src.set_property("location", fifo);
        let demux = make("matroskademux", "demux")?;
        let combiner = make("ndisinkcombiner", "combine")?;
        let sink = make("ndisink", "ndi")?;
        set_string(&sink, "ndi-name", &settings.name);

        pipeline
            .add_many([&src, &demux, &combiner, &sink])
            .map_err(|e| format!("could not assemble the NDI output: {e}"))?;
        gst::Element::link(&src, &demux)
            .map_err(|e| format!("could not link the programme FIFO to the demuxer: {e}"))?;
        gst::Element::link(&combiner, &sink)
            .map_err(|e| format!("could not link the combiner to ndisink: {e}"))?;

        // The programme arrives encoded and `ndisink` wants raw, so each stream
        // goes through its own decodebin. That decode is the price of NDI, not
        // of this plugin: nothing on the wire is in a form NDI accepts.
        let weak = pipeline.downgrade();
        let for_pads = reporter.clone();
        demux.connect_pad_added(move |_, pad| {
            let Some(pipeline) = weak.upgrade() else { return };
            if let Err(e) = decode_into_combiner(&pipeline, pad) {
                if let Some(r) = &for_pads {
                    r.error(format!("a programme stream could not be sent as NDI: {e}"));
                }
            }
        });

        let mut pipe = Pipe::wrap(pipeline);
        pipe.play(reporter.clone())?;
        if let Some(r) = &reporter {
            r.info(format!(
                "announcing the programme on the network as '{}'",
                settings.name
            ));
        }
        let stop = Arc::new(AtomicBool::new(false));
        let poller = spawn_poll(
            pipe.watch(),
            format!("the NDI sender '{}'", settings.name),
            reporter,
            Arc::clone(&stop),
        );
        Ok(Announcer { pipe, settings: settings.clone(), stop, poller })
    }

    pub fn health(&self) -> Health {
        health_of(
            &self.pipe.watch(),
            &format!("the NDI sender '{}'", self.settings.name),
        )
    }
}

impl Drop for Announcer {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(thread) = self.poller.take() {
            let _ = thread.join();
        }
        self.pipe.stop();
    }
}

/// Link a demuxed pad into a named element through a queue.
fn attach(pipeline: &gst::Pipeline, pad: &gst::Pad, into: &str) -> Result<(), String> {
    let target = pipeline
        .by_name(into)
        .ok_or_else(|| format!("the pipeline has lost its '{into}'"))?;
    let queue = make("queue", "")?;
    pipeline
        .add(&queue)
        .map_err(|e| format!("could not add a queue: {e}"))?;
    queue.sync_state_with_parent().ok();
    let sink_pad = queue.static_pad("sink").ok_or("the queue has no sink pad")?;
    pad.link(&sink_pad)
        .map_err(|e| format!("could not link a pad to its queue: {e}"))?;
    queue
        .link(&target)
        .map_err(|e| format!("could not link a stream into '{into}': {e}"))
}

/// Decode one programme stream and hand the raw frames to the combiner.
fn decode_into_combiner(pipeline: &gst::Pipeline, pad: &gst::Pad) -> Result<(), String> {
    let combiner = pipeline
        .by_name("combine")
        .ok_or("the pipeline has lost its combiner")?;
    let queue = make("queue", "")?;
    let decode = make("decodebin", "")?;
    let convert = make("videoconvert", "")?;
    let name = pad
        .current_caps()
        .and_then(|c| c.structure(0).map(|s| s.name().to_string()))
        .unwrap_or_default();
    let convert = if name.starts_with("audio/") {
        make("audioconvert", "")?
    } else {
        convert
    };
    for element in [&queue, &decode, &convert] {
        pipeline
            .add(element)
            .map_err(|e| format!("could not add an element to the NDI output: {e}"))?;
        element.sync_state_with_parent().ok();
    }
    gst::Element::link(&queue, &decode)
        .map_err(|e| format!("could not link the queue to the decoder: {e}"))?;
    let sink_pad = queue.static_pad("sink").ok_or("the queue has no sink pad")?;
    pad.link(&sink_pad)
        .map_err(|e| format!("could not link a programme pad: {e}"))?;

    // `decodebin` has its own dynamic pad; the rest of the branch is finished
    // when it appears.
    let convert_for_pad = convert.clone();
    decode.connect_pad_added(move |_, decoded| {
        if let Some(sink) = convert_for_pad.static_pad("sink") {
            let _ = decoded.link(&sink);
        }
    });
    convert
        .link(&combiner)
        .map_err(|e| format!("could not link a decoded stream into the combiner: {e}"))
}

fn spawn_poll(
    watch: gmx_netkit::pipe::Watch,
    what: String,
    reporter: Option<Reporter>,
    stop: Arc<AtomicBool>,
) -> Option<std::thread::JoinHandle<()>> {
    let reporter = reporter?;
    std::thread::Builder::new()
        .name("gmx-ndi-health".into())
        .spawn(move || {
            let mut last = String::new();
            while !stop.load(Ordering::Relaxed) {
                let health = health_of(&watch, &what);
                let state = format!("{:?}", health.state);
                if state != last {
                    last = state;
                    reporter.health_changed(health);
                } else {
                    reporter.set_health(health);
                }
                std::thread::sleep(std::time::Duration::from_millis(POLL_MS));
            }
        })
        .ok()
}

fn health_of(watch: &gmx_netkit::pipe::Watch, what: &str) -> Health {
    if let Some(failure) = watch.failure() {
        if failure.contains("NDI SDK") || failure.contains("NDI runtime") {
            return Health::failing(crate::library::missing());
        }
        return Health::failing(format!("{what} failed: {failure}"));
    }
    if watch.ended() {
        return Health::degraded(format!("{what} stopped sending"));
    }
    let mut health = Health::ok();
    health.detail = Some(format!("{what} is connected"));
    health
}

fn make(factory: &str, name: &str) -> Result<gst::Element, String> {
    let builder = gst::ElementFactory::make(factory);
    let builder = if name.is_empty() { builder } else { builder.name(name) };
    builder
        .build()
        .map_err(|e| format!("could not make '{factory}': {e}"))
}

fn set_string(element: &gst::Element, name: &str, value: &str) {
    if !value.is_empty() && element.find_property(name).is_some() {
        element.set_property(name, value);
    }
}

/// Set an enum property from its printed name, quietly doing nothing when this
/// build spells the value differently.
fn set_enum(element: &gst::Element, name: &str, value: &str) {
    if value.is_empty() || element.find_property(name).is_none() {
        return;
    }
    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        element.set_property_from_str(name, value);
    }));
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_source_with_no_sender_is_not_malformed_but_cannot_start() {
        let s = SourceSettings::from_params(&json!({}));
        assert!(s.malformed().is_none());
        assert!(s.problem().expect("no sender").contains("list_senders"));
    }

    #[test]
    fn a_bandwidth_that_is_not_one_of_the_three_names_them() {
        let s = SourceSettings::from_params(&json!({"name": "CAM", "bandwidth": "medium"}));
        let problem = s.malformed().expect("there is no medium");
        assert!(problem.contains("audio-only"), "{problem}");
    }

    #[test]
    fn an_address_alone_is_enough_for_a_sender_mdns_cannot_reach() {
        let s = SourceSettings::from_params(&json!({"address": "10.0.0.21:5961"}));
        assert!(s.problem().is_none());
        assert!(s.describe().contains("10.0.0.21"));
    }

    #[test]
    fn the_output_announces_itself_as_godwinmix_unless_told_otherwise() {
        assert_eq!(OutputSettings::from_params(&json!({})).name, "GodwinMix");
        assert_eq!(
            OutputSettings::from_params(&json!({"name": "Studio A"})).name,
            "Studio A"
        );
    }

    #[test]
    fn an_output_with_no_fifo_says_what_is_missing() {
        if !gmx_netkit::init().is_ok() || !gmx_netkit::elements::missing(OUTPUT_NEEDED).is_empty() {
            eprintln!("skipping: this build of GStreamer has no NDI sink");
            return;
        }
        let err = match Announcer::start(&OutputSettings::default(), "", None) {
            Ok(_) => panic!("an output with no FIFO must not start"),
            Err(e) => e,
        };
        assert!(err.contains("start.params.media"), "{err}");
    }

    #[test]
    fn a_source_for_a_sender_that_is_not_there_fails_with_the_runtime_message_or_the_sender() {
        if gmx_netkit::init().is_err() || !gmx_netkit::elements::missing(SOURCE_NEEDED).is_empty() {
            eprintln!("skipping: this build of GStreamer has no NDI plugin");
            return;
        }
        let path = std::env::temp_dir().join(format!("gmx-ndi-{}.mkv", std::process::id()));
        let settings = SourceSettings::from_params(&json!({"name": "NOBODY (NOTHING)"}));
        match Receiver::start(&settings, None, Some(path.clone())) {
            // With no runtime the state change fails and the message says so.
            Err(why) => assert!(
                why.contains("NDI sender") || why.contains("ndi.video"),
                "{why}"
            ),
            // With a runtime and no such sender it waits, which is also right.
            Ok(receiver) => {
                assert_ne!(receiver.health().state, godwinmix_sdk::wire::HealthState::Failing);
            }
        }
        let _ = std::fs::remove_file(&path);
    }
}
