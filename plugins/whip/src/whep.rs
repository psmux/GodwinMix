//! `whip/whep`: watch a WebRTC stream as a source.
//!
//! WHEP is WHIP pointed the other way: the same HTTP POST of an SDP offer to an
//! endpoint URL, except the media comes back. It is how a browser based guest,
//! a cloud encoder or another mixer's WHIP output arrives here.
//!
//! ```text
//!   whepsrc ──► matroskamux ──► fdsink fd=1
//! ```
//!
//! `whepsrc` hands over RTP depayloaded streams, so unlike `srt/source` there
//! is something to mux before the bytes can cross: the core's container
//! transport wants one container, and streamable Matroska is what it is.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use gmx_netkit::pipe::Pipe;
use godwinmix_sdk::plugin::Reporter;
use godwinmix_sdk::wire::Health;
use gstreamer as gst;
use gstreamer::prelude::*;

use crate::settings::Settings;

/// The WHEP receivers this plugin knows, best first.
///
/// `whepsrc` hands over the encoded streams, which is what we want: they cross
/// to the core as they arrived and are decoded once, there. `whepclientsrc` is
/// its replacement and decodes inside itself, so it costs a decode here and a
/// much fatter pipe. GStreamer 1.28 deprecates the first in favour of the
/// second and prints a warning saying so; until the cheap one is actually gone,
/// it is the one to use.
const RECEIVERS: &[&str] = &["whepsrc", "whepclientsrc"];

/// What this provide needs from GStreamer, besides one of [`RECEIVERS`].
pub const NEEDED: &[&str] = &["matroskamux"];

/// The first WHEP receiver this build has.
pub fn receiver() -> Option<&'static str> {
    RECEIVERS.iter().copied().find(|e| gmx_netkit::elements::exists(e))
}

/// The message for a build with no WHEP receiver at all.
pub fn no_receiver() -> String {
    format!(
        "this build of GStreamer has no WHEP receiver: neither {} is registered. \
         They come from {}. Until one is there, receive over SRT with srt/source \
         or over RTMP with ingest/rtmp instead.",
        RECEIVERS.join(" nor "),
        gmx_netkit::elements::where_from("whepsrc")
    )
}

const POLL_MS: u64 = 1000;

/// A running WHEP receiver.
pub struct Watcher {
    pipe: Pipe,
    stop: Arc<AtomicBool>,
    poller: Option<std::thread::JoinHandle<()>>,
}

impl Watcher {
    pub fn start(settings: &Settings, reporter: Option<Reporter>) -> Result<Watcher, String> {
        Watcher::start_into(settings, reporter, None)
    }

    /// The same, writing to a file instead of stdout, which is what the tests
    /// want: a test harness owns its own stdout.
    pub fn start_into(
        settings: &Settings,
        reporter: Option<Reporter>,
        file: Option<std::path::PathBuf>,
    ) -> Result<Watcher, String> {
        gmx_netkit::init()?;
        gmx_netkit::elements::require(NEEDED)?;
        let factory = receiver().ok_or_else(no_receiver)?;

        let pipeline = gst::Pipeline::with_name("gmx-whep-source");
        let src = make(factory, "whep")?;
        // `whepsrc` carries these on itself; `whepclientsrc` carries them on a
        // signaller child object. Set whichever this build has.
        set_string(&src, "whep-endpoint", &settings.endpoint);
        set_string(&src, "auth-token", &settings.token);
        set_string(&src, "stun-server", &settings.stun_server);
        set_string(&src, "turn-server", &settings.turn_server);
        if src.find_property("timeout").is_some() {
            src.set_property("timeout", settings.timeout_secs);
        }

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
        if sink.find_property("sync").is_some() {
            sink.set_property("sync", false);
        }
        if sink.find_property("async").is_some() {
            sink.set_property("async", false);
        }

        pipeline
            .add_many([&src, &mux, &sink])
            .map_err(|e| format!("could not assemble the WHEP pipeline: {e}"))?;
        gst::Element::link(&mux, &sink)
            .map_err(|e| format!("could not link the muxer to the pipe: {e}"))?;

        // `whepsrc` has sometimes pads: video and audio appear when the answer
        // has been negotiated, so each is linked to the muxer as it arrives.
        let weak = pipeline.downgrade();
        let reporter_for_pads = reporter.clone();
        src.connect_pad_added(move |_, pad| {
            let Some(pipeline) = weak.upgrade() else { return };
            if let Err(e) = attach(&pipeline, pad) {
                if let Some(r) = &reporter_for_pads {
                    r.error(format!("a WHEP stream could not be muxed: {e}"));
                }
            }
        });

        let mut pipe = Pipe::wrap(pipeline);
        // A WHEP session is negotiated during the state change, so an endpoint
        // that is not there fails here rather than later. That is an error the
        // core's supervisor is meant to see: it restarts the instance with its
        // own backoff, which is where a retry loop belongs. `restart-in-place`
        // is declared for exactly this.
        pipe.play(reporter.clone()).map_err(|e| {
            format!(
                "{e}. The WHEP endpoint {} did not complete a session. Check the URL \
                 answers a POST, then the token, then that something is publishing to it.",
                settings.redacted_endpoint()
            )
        })?;
        let stop = Arc::new(AtomicBool::new(false));
        let poller = spawn_poll(pipe.watch(), src, reporter, Arc::clone(&stop));
        Ok(Watcher { pipe, stop, poller })
    }

    pub fn health(&self) -> Health {
        let src = self.pipe.by_name("whep");
        health_of(&self.pipe.watch(), src.as_ref())
    }
}

impl Drop for Watcher {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(thread) = self.poller.take() {
            let _ = thread.join();
        }
        self.pipe.stop();
    }
}

/// Link one negotiated stream into the muxer, through a queue so a slow pipe
/// never stalls the depayloader.
fn attach(pipeline: &gst::Pipeline, pad: &gst::Pad) -> Result<(), String> {
    let mux = pipeline
        .by_name("mux")
        .ok_or("the pipeline has lost its muxer")?;
    let queue = make("queue", "")?;
    pipeline
        .add(&queue)
        .map_err(|e| format!("could not add a queue: {e}"))?;
    queue.sync_state_with_parent().ok();
    let sink_pad = queue.static_pad("sink").ok_or("the queue has no sink pad")?;
    pad.link(&sink_pad)
        .map_err(|e| format!("could not link a WHEP pad to its queue: {e}"))?;
    queue
        .link(&mux)
        .map_err(|e| format!("could not link a WHEP stream into the muxer: {e}"))
}

fn spawn_poll(
    watch: gmx_netkit::pipe::Watch,
    src: gst::Element,
    reporter: Option<Reporter>,
    stop: Arc<AtomicBool>,
) -> Option<std::thread::JoinHandle<()>> {
    let reporter = reporter?;
    std::thread::Builder::new()
        .name("gmx-whep-health".into())
        .spawn(move || {
            let mut last = String::new();
            while !stop.load(Ordering::Relaxed) {
                let health = health_of(&watch, Some(&src));
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

fn health_of(watch: &gmx_netkit::pipe::Watch, src: Option<&gst::Element>) -> Health {
    if let Some(failure) = watch.failure() {
        return Health::failing(format!(
            "{failure}. Check the WHEP endpoint URL and the token, then that something \
             is actually publishing to it."
        ));
    }
    let state = src
        .and_then(|s| gmx_netkit::stats::find_with_property(s, "ice-connection-state"))
        .and_then(|e| gmx_netkit::stats::enum_name(&e, "ice-connection-state"));
    match state {
        Some(s) if s.contains("connected") || s.contains("completed") => {
            let mut health = Health::ok();
            health.detail = Some(format!("ice {s}"));
            health
        }
        Some(s) => Health::degraded(format!(
            "ice {s}: the offer is with the endpoint and the media path is not up yet."
        )),
        None => Health::degraded("connecting to the WHEP endpoint"),
    }
}

fn make(factory: &str, name: &str) -> Result<gst::Element, String> {
    let builder = gst::ElementFactory::make(factory);
    let builder = if name.is_empty() { builder } else { builder.name(name) };
    builder
        .build()
        .map_err(|e| format!("could not make '{factory}': {e}"))
}

/// Set a string property on the element, or on its signaller child if that is
/// where this build keeps it. An empty value is never written.
fn set_string(element: &gst::Element, name: &str, value: &str) {
    if value.is_empty() {
        return;
    }
    if element.find_property(name).is_some() {
        element.set_property(name, value);
        return;
    }
    if element.find_property("signaller").is_none() {
        return;
    }
    let signaller = element.property::<glib::Object>("signaller");
    if signaller.find_property(name).is_some() {
        signaller.set_property(name, value);
    }
}

/// Read a string property back from wherever it was set. For the tests.
#[cfg(test)]
fn get_string(element: &gst::Element, name: &str) -> Option<String> {
    if element.find_property(name).is_some() {
        return element.property::<Option<String>>(name);
    }
    element.find_property("signaller")?;
    let signaller = element.property::<glib::Object>("signaller");
    signaller.find_property(name)?;
    signaller.property::<Option<String>>(name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn have_whep() -> bool {
        gmx_netkit::init().is_ok()
            && gmx_netkit::elements::missing(NEEDED).is_empty()
            && receiver().is_some()
    }

    #[test]
    fn an_endpoint_that_is_not_there_fails_start_with_a_message_naming_it() {
        if !have_whep() {
            eprintln!("skipping: {}", no_receiver());
            return;
        }
        let path = std::env::temp_dir().join(format!("gmx-whep-test-{}.mkv", std::process::id()));
        let settings = Settings::from_params(&json!({
            "endpoint": "http://127.0.0.1:1/whep", "timeout_secs": 1
        }));
        // Where the receiver negotiates inside the state change, the refusal
        // comes back from `start_into` and names the endpoint. Where it
        // negotiates on a thread of its own the state change succeeds and the
        // refusal arrives on the bus a moment later, which is what Windows
        // does. Both are the same answer to an operator: this endpoint is not
        // carrying media. Waiting for the second is bounded, because a
        // receiver that never reports anything is a failure too.
        match Watcher::start_into(&settings, None, Some(path.clone())) {
            Err(err) => {
                assert!(err.contains("127.0.0.1:1"), "{err}");
                assert!(err.contains("POST"), "{err}");
            }
            Ok(watcher) => {
                let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
                loop {
                    let health = watcher.health();
                    if health.state == godwinmix_sdk::wire::HealthState::Failing {
                        break;
                    }
                    assert!(
                        std::time::Instant::now() < deadline,
                        "nothing is listening on 127.0.0.1:1, so the receiver has to go \
                         failing; 30 s after start it still reports {:?} ({:?})",
                        health.state,
                        health.detail
                    );
                    std::thread::sleep(std::time::Duration::from_millis(100));
                }
            }
        }
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn the_endpoint_reaches_whichever_element_this_build_has() {
        if !have_whep() {
            eprintln!("skipping: {}", no_receiver());
            return;
        }
        let src = make(receiver().expect("a receiver"), "whep").expect("the WHEP receiver");
        set_string(&src, "whep-endpoint", "https://example.com/whep/room");
        assert_eq!(
            get_string(&src, "whep-endpoint").unwrap_or_default(),
            "https://example.com/whep/room"
        );
    }

    #[test]
    fn the_message_for_a_build_with_no_receiver_names_the_way_forward() {
        let message = no_receiver();
        assert!(message.contains("srt/source"), "{message}");
        assert!(message.contains("whepsrc"), "{message}");
    }
}
