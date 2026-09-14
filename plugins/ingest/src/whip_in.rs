//! `ingest/whip`: the mixer runs the WHIP endpoint.
//!
//! `whipserversrc` is a WHIP server in an element: it runs a small HTTP server,
//! answers the publisher's POST with an SDP answer, and hands the negotiated
//! streams out on sometimes pads. A browser needs nothing but the URL, which is
//! why this is the shortest path from "a guest with a laptop" to "a source on
//! the mixer".
//!
//! ```text
//!   whipserversrc ──► matroskamux ──► fdsink fd=1
//! ```
//!
//! It arrived in the GStreamer 1.28 rs webrtc set. A build without it is
//! refused with a message naming the package and what to use instead, rather
//! than a missing element error.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use gmx_netkit::pipe::Pipe;
use godwinmix_sdk::plugin::Reporter;
use godwinmix_sdk::wire::Health;
use gstreamer as gst;
use gstreamer::prelude::*;
use serde_json::Value;

/// What this provide needs from GStreamer besides the base set.
pub const NEEDED: &[&str] = &["whipserversrc", "matroskamux"];

const POLL_MS: u64 = 1000;

/// The message for a build with no WHIP server element.
pub fn unavailable() -> String {
    format!(
        "this build of GStreamer has no 'whipserversrc', so the mixer cannot be a WHIP \
         endpoint. It comes from {}. Until it is there, take the stream over RTMP with \
         ingest/rtmp, which needs no GStreamer element at all, or over SRT with \
         srt/source in listener mode.",
        gmx_netkit::elements::where_from("whipserversrc")
    )
}

/// The settings of `ingest/whip`.
#[derive(Debug, Clone, PartialEq)]
pub struct Settings {
    pub bind: String,
    pub port: u16,
    pub path: String,
    pub stun_server: String,
}

impl Default for Settings {
    fn default() -> Settings {
        Settings {
            bind: "0.0.0.0".into(),
            // 8889 is what mediamtx uses for WHIP, so an operator moving off it
            // does not have to change the address in everybody's browser.
            port: 8889,
            path: "/whip".into(),
            stun_server: String::new(),
        }
    }
}

impl Settings {
    pub fn from_params(params: &Value) -> Settings {
        let d = Settings::default();
        let string = |key: &str| {
            params
                .get(key)
                .and_then(Value::as_str)
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
        };
        Settings {
            bind: string("bind").unwrap_or(d.bind),
            port: params
                .get("port")
                .and_then(Value::as_u64)
                .and_then(|p| u16::try_from(p).ok())
                .unwrap_or(d.port),
            path: string("path").unwrap_or(d.path),
            stun_server: string("stun_server").unwrap_or(d.stun_server),
        }
    }

    pub fn problem(&self) -> Option<String> {
        if !self.path.starts_with('/') {
            return Some(format!(
                "path is '{}'. It is the part of the URL after the host, so it begins \
                 with a slash: /whip.",
                self.path
            ));
        }
        if !self.stun_server.is_empty() && !self.stun_server.to_lowercase().starts_with("stun://") {
            return Some(format!(
                "stun_server is '{}'. Write it as stun://host:port, or leave it empty.",
                self.stun_server
            ));
        }
        None
    }

    /// The address a publisher is given.
    pub fn publish_url(&self) -> String {
        format!("http://<this machine>:{}{}", self.port, self.path)
    }

    /// What `whipserversrc` wants as its listening address.
    fn host_addr(&self) -> String {
        format!("http://{}:{}", self.bind, self.port)
    }
}

/// A running WHIP endpoint.
pub struct Endpoint {
    pipe: Pipe,
    settings: Settings,
    stop: Arc<AtomicBool>,
    poller: Option<std::thread::JoinHandle<()>>,
}

impl Endpoint {
    pub fn start(settings: &Settings, reporter: Option<Reporter>) -> Result<Endpoint, String> {
        Endpoint::start_into(settings, reporter, None)
    }

    pub fn start_into(
        settings: &Settings,
        reporter: Option<Reporter>,
        file: Option<std::path::PathBuf>,
    ) -> Result<Endpoint, String> {
        gmx_netkit::init()?;
        if !gmx_netkit::elements::exists("whipserversrc") {
            return Err(unavailable());
        }
        gmx_netkit::elements::require(NEEDED)?;

        let pipeline = gst::Pipeline::with_name("gmx-ingest-whip");
        let src = make("whipserversrc", "whip")?;
        let signaller = src.property::<glib::Object>("signaller");
        if signaller.find_property("host-addr").is_none() {
            return Err(
                "this build of whipserversrc has no 'host-addr' on its signaller, so the \
                 endpoint cannot be told where to listen. GStreamer 1.28 or newer is what \
                 this plugin is written against."
                    .into(),
            );
        }
        signaller.set_property("host-addr", settings.host_addr());
        if !settings.path.is_empty() && signaller.find_property("path").is_some() {
            signaller.set_property("path", &settings.path);
        }
        if !settings.stun_server.is_empty() && src.find_property("stun-server").is_some() {
            src.set_property("stun-server", &settings.stun_server);
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
        for name in ["sync", "async"] {
            if sink.find_property(name).is_some() {
                sink.set_property(name, false);
            }
        }

        pipeline
            .add_many([&src, &mux, &sink])
            .map_err(|e| format!("could not assemble the WHIP endpoint: {e}"))?;
        gst::Element::link(&mux, &sink)
            .map_err(|e| format!("could not link the muxer to the pipe: {e}"))?;

        let weak = pipeline.downgrade();
        let for_pads = reporter.clone();
        src.connect_pad_added(move |_, pad| {
            let Some(pipeline) = weak.upgrade() else { return };
            if let Err(e) = attach(&pipeline, pad) {
                if let Some(r) = &for_pads {
                    r.error(format!("a published WHIP stream could not be muxed: {e}"));
                }
            }
        });

        let mut pipe = Pipe::wrap(pipeline);
        pipe.play(reporter.clone()).map_err(|e| {
            format!(
                "{e}. The WHIP endpoint could not listen on {}. Another process may have \
                 the port.",
                settings.host_addr()
            )
        })?;
        if let Some(r) = &reporter {
            r.info(format!("publish to {} to appear here", settings.publish_url()));
        }
        let stop = Arc::new(AtomicBool::new(false));
        let poller = spawn_poll(pipe.watch(), src, settings.clone(), reporter, Arc::clone(&stop));
        Ok(Endpoint { pipe, settings: settings.clone(), stop, poller })
    }

    pub fn health(&self) -> Health {
        let src = self.pipe.by_name("whip");
        health_of(&self.pipe.watch(), src.as_ref(), &self.settings)
    }

    pub fn stats(&self) -> Value {
        serde_json::json!({
            "address": self.settings.publish_url(),
            "health": self.health().detail,
        })
    }
}

impl Drop for Endpoint {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(thread) = self.poller.take() {
            let _ = thread.join();
        }
        self.pipe.stop();
    }
}

fn attach(pipeline: &gst::Pipeline, pad: &gst::Pad) -> Result<(), String> {
    let mux = pipeline.by_name("mux").ok_or("the pipeline has lost its muxer")?;
    let queue = make("queue", "")?;
    pipeline
        .add(&queue)
        .map_err(|e| format!("could not add a queue: {e}"))?;
    queue.sync_state_with_parent().ok();
    let sink_pad = queue.static_pad("sink").ok_or("the queue has no sink pad")?;
    pad.link(&sink_pad)
        .map_err(|e| format!("could not link a published pad to its queue: {e}"))?;
    queue
        .link(&mux)
        .map_err(|e| format!("could not link a published stream into the muxer: {e}"))
}

fn spawn_poll(
    watch: gmx_netkit::pipe::Watch,
    src: gst::Element,
    settings: Settings,
    reporter: Option<Reporter>,
    stop: Arc<AtomicBool>,
) -> Option<std::thread::JoinHandle<()>> {
    let reporter = reporter?;
    std::thread::Builder::new()
        .name("gmx-ingest-whip-health".into())
        .spawn(move || {
            let mut last = String::new();
            while !stop.load(Ordering::Relaxed) {
                let health = health_of(&watch, Some(&src), &settings);
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

fn health_of(
    watch: &gmx_netkit::pipe::Watch,
    src: Option<&gst::Element>,
    settings: &Settings,
) -> Health {
    if let Some(failure) = watch.failure() {
        return Health::failing(failure);
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
        _ => Health::degraded(format!(
            "nobody is publishing. Point a WHIP publisher at {} and the picture appears \
             as soon as the session is up.",
            settings.publish_url()
        )),
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

    fn available() -> bool {
        gmx_netkit::init().is_ok() && gmx_netkit::elements::missing(NEEDED).is_empty()
    }

    fn free_port() -> u16 {
        std::net::TcpListener::bind("127.0.0.1:0")
            .expect("a free port")
            .local_addr()
            .expect("an address")
            .port()
    }

    #[test]
    fn the_defaults_listen_where_mediamtx_did() {
        let s = Settings::from_params(&json!({}));
        assert_eq!(s.port, 8889);
        assert_eq!(s.path, "/whip");
        assert!(s.problem().is_none());
    }

    #[test]
    fn a_path_with_no_slash_is_refused_and_shows_the_shape() {
        let s = Settings::from_params(&json!({"path": "whip"}));
        assert!(s.problem().expect("a path starts with /").contains("/whip"));
    }

    #[test]
    fn the_message_for_a_build_with_no_element_names_what_to_use_instead() {
        let message = unavailable();
        assert!(message.contains("ingest/rtmp"), "{message}");
        assert!(message.contains("srt/source"), "{message}");
    }

    #[test]
    fn an_endpoint_comes_up_on_a_free_port_and_waits() {
        if !available() {
            eprintln!("skipping: {}", unavailable());
            return;
        }
        let path = std::env::temp_dir().join(format!("gmx-whipin-{}.mkv", std::process::id()));
        let settings = Settings::from_params(&json!({
            "bind": "127.0.0.1", "port": free_port()
        }));
        let endpoint = Endpoint::start_into(&settings, None, Some(path.clone()))
            .expect("a WHIP endpoint on a free port comes up");
        let health = endpoint.health();
        assert_eq!(health.state, godwinmix_sdk::wire::HealthState::Degraded);
        assert!(health.detail.unwrap_or_default().contains("http://"));
        drop(endpoint);
        let _ = std::fs::remove_file(&path);
    }
}
