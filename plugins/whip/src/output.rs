//! `whip/output`: the programme to a WHIP endpoint.
//!
//! The core hands an output a FIFO carrying streamable Matroska with the
//! programme already encoded (H.264 video, AAC audio). That is the one place in
//! the plugin contract where the media flows towards the plugin, and it is
//! `start.params.media` and `GMX_MEDIA` both.
//!
//! ```text
//!   filesrc(fifo) ──► matroskademux ──┬─► h264parse ───────────────► whipclientsink.video_0
//!                                     └─► aacparse ! decode ! conv ─► whipclientsink.audio_0
//! ```
//!
//! The video is passed through: `whipclientsink` accepts `video/x-h264` on its
//! request pad, so the programme's encode is the only encode. The audio is not:
//! WebRTC has no AAC, so it is decoded and handed over raw for the sink to make
//! Opus of. One audio decode is the price of the protocol, not of this plugin.

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

use gmx_netkit::backoff::Backoff;
use gmx_netkit::pipe::Pipe;
use godwinmix_sdk::plugin::Reporter;
use godwinmix_sdk::wire::{Health, HealthState};
use gstreamer as gst;
use gstreamer::prelude::*;

use crate::settings::Settings;

/// What the plugin needs from GStreamer to send at all.
pub const NEEDED: &[&str] = &["whipclientsink", "matroskademux", "h264parse"];

/// How often the supervisor thread looks at the pipeline.
const TICK_MS: u64 = 250;

/// A WHIP sender that rebuilds itself when the endpoint goes away.
pub struct Sender {
    shared: Arc<Shared>,
    stop: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}

/// What the supervisor thread and the calling thread both touch.
struct Shared {
    settings: Settings,
    fifo: String,
    reporter: Option<Reporter>,
    /// The live pipeline, replaced on every reconnect.
    pipe: Mutex<Option<Pipe>>,
    reconnects: AtomicU32,
}

impl Sender {
    /// Open the FIFO and start sending. Returns as soon as the pipeline is
    /// built; the endpoint may still be being dialled.
    pub fn start(
        settings: &Settings,
        fifo: &str,
        reporter: Option<Reporter>,
    ) -> Result<Sender, String> {
        gmx_netkit::init()?;
        gmx_netkit::elements::require(NEEDED)?;
        if fifo.trim().is_empty() {
            return Err("the core gave no media address. An output reads the programme \
                        from the FIFO named in start.params.media; without it there is \
                        nothing to send."
                .into());
        }
        let shared = Arc::new(Shared {
            settings: settings.clone(),
            fifo: fifo.to_string(),
            reporter: reporter.clone(),
            pipe: Mutex::new(None),
            reconnects: AtomicU32::new(0),
        });
        // The first build is done here so `start` fails loudly when the
        // pipeline cannot be assembled at all, rather than looking healthy and
        // retrying a description that will never work.
        let first = build(&shared)?;
        *shared.pipe.lock().unwrap_or_else(|e| e.into_inner()) = Some(first);

        let stop = Arc::new(AtomicBool::new(false));
        let thread = spawn_supervisor(Arc::clone(&shared), Arc::clone(&stop));
        Ok(Sender { shared, stop, thread })
    }

    /// What the connection is doing, read fresh.
    pub fn health(&self) -> Health {
        self.shared.health()
    }

    /// How many times the endpoint has been dialled again since `start`.
    pub fn reconnects(&self) -> u32 {
        self.shared.reconnects.load(Ordering::Relaxed)
    }
}

impl Drop for Sender {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
        if let Some(mut pipe) = self.shared.pipe.lock().unwrap_or_else(|e| e.into_inner()).take() {
            pipe.stop();
        }
    }
}

impl Shared {
    fn health(&self) -> Health {
        let held = self.pipe.lock().unwrap_or_else(|e| e.into_inner());
        let Some(pipe) = held.as_ref() else {
            return Health::failing(
                "not connected. The endpoint is being dialled again; watch health.changed.",
            );
        };
        if let Some(failure) = pipe.failure() {
            return Health::degraded(format!(
                "the WHIP endpoint refused or dropped the connection ({failure}). \
                 Dialling again."
            ));
        }
        match self.ice_state(pipe) {
            Some(state) if state.contains("connected") || state.contains("completed") => {
                let mut health = Health::ok();
                health.detail = Some(format!("ice {state}"));
                health
            }
            Some(state) => Health::degraded(format!(
                "ice {state}: the offer has gone to the endpoint and the media path is \
                 not up yet. A TURN server is what fixes this when it never comes up."
            )),
            None => Health::degraded(
                "connecting. This build of whipclientsink does not report an ICE state, \
                 so there is nothing more to say until media flows.",
            ),
        }
    }

    /// The ICE state, read off whichever element inside the sink has it.
    fn ice_state(&self, pipe: &Pipe) -> Option<String> {
        let sink = pipe.by_name("whip")?;
        let holder = gmx_netkit::stats::find_with_property(&sink, "ice-connection-state")?;
        gmx_netkit::stats::enum_name(&holder, "ice-connection-state")
    }
}

/// Build the pipeline for one attempt and set it playing.
fn build(shared: &Arc<Shared>) -> Result<Pipe, String> {
    let s = &shared.settings;
    let pipeline = gst::Pipeline::with_name("gmx-whip-output");

    let src = make("filesrc", "fifo")?;
    src.set_property("location", &shared.fifo);
    let demux = make("matroskademux", "demux")?;

    let sink = make("whipclientsink", "whip")?;
    configure_signaller(&sink, s)?;
    if !s.stun_server.is_empty() && sink.find_property("stun-server").is_some() {
        sink.set_property("stun-server", &s.stun_server);
    }
    if !s.turn_server.is_empty() && sink.find_property("turn-servers").is_some() {
        let list = gst::Array::new([s.turn_server.to_send_value()]);
        sink.set_property("turn-servers", list);
    }

    pipeline
        .add_many([&src, &demux, &sink])
        .map_err(|e| format!("could not assemble the WHIP pipeline: {e}"))?;
    gst::Element::link(&src, &demux)
        .map_err(|e| format!("could not link the programme FIFO to the demuxer: {e}"))?;

    // Matroska pads appear when the demuxer has read the header, which is after
    // the FIFO has bytes in it, so the branches are built here rather than up
    // front. Everything this closure does is element creation and linking; it
    // never blocks and never calls back into the plugin.
    let weak = pipeline.downgrade();
    let reporter = shared.reporter.clone();
    demux.connect_pad_added(move |_, pad| {
        let Some(pipeline) = weak.upgrade() else { return };
        if let Err(e) = branch(&pipeline, pad) {
            if let Some(r) = &reporter {
                r.error(format!("a programme stream could not be connected: {e}"));
            }
        }
    });

    let mut pipe = Pipe::wrap(pipeline);
    pipe.play(shared.reporter.clone())?;
    Ok(pipe)
}

/// The endpoint and the bearer token live on the sink's signaller object.
fn configure_signaller(sink: &gst::Element, s: &Settings) -> Result<(), String> {
    let signaller = sink.property::<glib::Object>("signaller");
    set_string(&signaller, "whip-endpoint", &s.endpoint)?;
    if !s.token.is_empty() {
        set_string(&signaller, "auth-token", &s.token)?;
    }
    if s.timeout_secs > 0 && signaller.find_property("timeout").is_some() {
        signaller.set_property("timeout", s.timeout_secs);
    }
    Ok(())
}

fn set_string(object: &glib::Object, name: &str, value: &str) -> Result<(), String> {
    if object.find_property(name).is_none() {
        return Err(format!(
            "this build of whipclientsink has no '{name}' on its signaller, so the \
             endpoint cannot be set. Check the GStreamer version: the rs webrtc set \
             from 1.28 is what this plugin is written against."
        ));
    }
    object.set_property(name, value);
    Ok(())
}

/// Connect one demuxed programme stream to the sink.
fn branch(pipeline: &gst::Pipeline, pad: &gst::Pad) -> Result<(), String> {
    let caps = pad.current_caps().ok_or("a pad appeared with no caps")?;
    let name = caps
        .structure(0)
        .map(|s| s.name().to_string())
        .unwrap_or_default();
    let sink = pipeline
        .by_name("whip")
        .ok_or("the pipeline has lost its whipclientsink")?;

    let chain: Vec<gst::Element> = if name.starts_with("video/x-h264") {
        // Straight through. The programme's encode is the only encode.
        vec![
            make("queue", "")?,
            configured_h264parse()?,
            make("capsfilter", "")?,
        ]
    } else if name.starts_with("audio/") {
        // WebRTC has no AAC. Decode and hand over raw; the sink makes Opus.
        vec![
            make("queue", "")?,
            make("decodebin", "")?,
            make("audioconvert", "")?,
            make("audioresample", "")?,
        ]
    } else if name.starts_with("video/") {
        return Err(format!(
            "the programme carries '{name}', which whipclientsink cannot take without \
             re-encoding. Set the programme's video codec to H.264 in codecs.toml."
        ));
    } else {
        return Ok(());
    };

    for element in &chain {
        pipeline
            .add(element)
            .map_err(|e| format!("could not add an element to the WHIP pipeline: {e}"))?;
        element.sync_state_with_parent().ok();
    }
    link_chain(&chain).map_err(|e| format!("could not link the {name} branch: {e}"))?;
    let entry = chain.first().ok_or("an empty branch")?;
    let exit = chain.last().ok_or("an empty branch")?;
    pad.link(&entry.static_pad("sink").ok_or("no sink pad on the branch")?)
        .map_err(|e| format!("could not link the demuxer pad: {e}"))?;
    // `decodebin` in the audio branch has its own dynamic pad, so that branch
    // is finished by the same mechanism one level down.
    if exit.factory().map(|f| f.name() == "decodebin").unwrap_or(false) {
        return Ok(());
    }
    exit.link(&sink)
        .map_err(|e| format!("could not link the {name} branch to whipclientsink: {e}"))
}

fn link_chain(chain: &[gst::Element]) -> Result<(), glib::BoolError> {
    for pair in chain.windows(2) {
        if pair[0].factory().map(|f| f.name() == "decodebin").unwrap_or(false) {
            let next = pair[1].clone();
            pair[0].connect_pad_added(move |_, pad| {
                if let Some(sink) = next.static_pad("sink") {
                    let _ = pad.link(&sink);
                }
            });
            continue;
        }
        gst::Element::link(&pair[0], &pair[1])?;
    }
    Ok(())
}

fn configured_h264parse() -> Result<gst::Element, String> {
    let parse = make("h264parse", "")?;
    // Repeat the SPS and PPS on every keyframe. A viewer joining after a
    // reconnect then has what it needs without waiting for the next one.
    if parse.find_property("config-interval").is_some() {
        parse.set_property("config-interval", -1i32);
    }
    Ok(parse)
}

/// Watch the pipeline, and dial again with a backoff when it fails.
fn spawn_supervisor(
    shared: Arc<Shared>,
    stop: Arc<AtomicBool>,
) -> Option<std::thread::JoinHandle<()>> {
    std::thread::Builder::new()
        .name("gmx-whip-supervise".into())
        .spawn(move || {
            let mut backoff =
                Backoff::with(shared.settings.reconnect_first_ms, shared.settings.reconnect_max_ms);
            let mut last_state = HealthState::Degraded;
            while !stop.load(Ordering::Relaxed) {
                std::thread::sleep(std::time::Duration::from_millis(TICK_MS));
                let health = shared.health();
                if health.state != last_state {
                    last_state = health.state;
                    if let Some(r) = &shared.reporter {
                        r.health_changed(health.clone());
                    }
                } else if let Some(r) = &shared.reporter {
                    r.set_health(health.clone());
                }
                if health.state == HealthState::Ok {
                    backoff.reset();
                    continue;
                }
                let broken = shared
                    .pipe
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .as_ref()
                    .map(|p| p.failure().is_some())
                    .unwrap_or(true);
                if !broken {
                    continue;
                }
                let wait = backoff.take();
                if let Some(r) = &shared.reporter {
                    r.warn(format!(
                        "the WHIP endpoint is not taking the programme. Dialling again in \
                         {} ms (attempt {}).",
                        wait.as_millis(),
                        backoff.attempts()
                    ));
                }
                if sleep_unless_stopped(&stop, wait) {
                    return;
                }
                reconnect(&shared);
            }
        })
        .ok()
}

/// Sleep in small pieces so `stop` is honoured inside a long backoff.
fn sleep_unless_stopped(stop: &Arc<AtomicBool>, wait: std::time::Duration) -> bool {
    let mut left = wait;
    let slice = std::time::Duration::from_millis(TICK_MS);
    while !left.is_zero() {
        if stop.load(Ordering::Relaxed) {
            return true;
        }
        let step = left.min(slice);
        std::thread::sleep(step);
        left -= step;
    }
    stop.load(Ordering::Relaxed)
}

/// Tear the pipeline down and build a fresh one.
fn reconnect(shared: &Arc<Shared>) {
    if let Some(mut old) = shared.pipe.lock().unwrap_or_else(|e| e.into_inner()).take() {
        old.stop();
    }
    shared.reconnects.fetch_add(1, Ordering::Relaxed);
    match build(shared) {
        Ok(fresh) => {
            *shared.pipe.lock().unwrap_or_else(|e| e.into_inner()) = Some(fresh);
        }
        Err(e) => {
            if let Some(r) = &shared.reporter {
                r.error(format!("the WHIP pipeline would not rebuild: {e}"));
            }
        }
    }
}

fn make(factory: &str, name: &str) -> Result<gst::Element, String> {
    let builder = gst::ElementFactory::make(factory);
    let builder = if name.is_empty() { builder } else { builder.name(name) };
    builder
        .build()
        .map_err(|e| format!("could not make '{factory}': {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn have_whip() -> bool {
        gmx_netkit::init().is_ok() && gmx_netkit::elements::missing(NEEDED).is_empty()
    }

    #[test]
    fn a_start_with_no_media_address_says_what_is_missing() {
        if !have_whip() {
            eprintln!("skipping: this build of GStreamer has no whipclientsink");
            return;
        }
        let settings = Settings::from_params(&json!({"endpoint": "https://example.com/whip"}));
        let err = match Sender::start(&settings, "", None) {
            Ok(_) => panic!("an output with no FIFO must not start"),
            Err(e) => e,
        };
        assert!(err.contains("start.params.media"), "{err}");
    }

    #[test]
    fn the_endpoint_and_the_token_reach_the_signaller() {
        if !have_whip() {
            eprintln!("skipping: this build of GStreamer has no whipclientsink");
            return;
        }
        let sink = make("whipclientsink", "whip").expect("whipclientsink");
        let settings = Settings::from_params(&json!({
            "endpoint": "https://example.com/whip/studio", "token": "a-token"
        }));
        configure_signaller(&sink, &settings).expect("the signaller takes both");
        let signaller = sink.property::<glib::Object>("signaller");
        assert_eq!(
            signaller.property::<Option<String>>("whip-endpoint").unwrap_or_default(),
            "https://example.com/whip/studio"
        );
    }

    #[test]
    fn a_pipeline_over_a_real_fifo_builds_and_reports_a_state() {
        if !have_whip() {
            eprintln!("skipping: this build of GStreamer has no whipclientsink");
            return;
        }
        // A plain file stands in for the FIFO: `filesrc` does not care, and the
        // point of the check is that the elements assemble and the supervisor
        // answers rather than panicking with nowhere to send.
        let path = std::env::temp_dir().join(format!("gmx-whip-test-{}.mkv", std::process::id()));
        std::fs::write(&path, b"not really matroska").expect("a stand in for the FIFO");
        let settings = Settings::from_params(&json!({
            "endpoint": "http://127.0.0.1:1/whip", "reconnect_first_ms": 60_000
        }));
        let sender = Sender::start(&settings, &path.to_string_lossy(), None)
            .expect("the pipeline assembles even when the endpoint is unreachable");
        let health = sender.health();
        assert_ne!(health.state, HealthState::Ok, "nothing is listening on port 1");
        assert!(health.detail.is_some());
        drop(sender);
        let _ = std::fs::remove_file(&path);
    }
}
