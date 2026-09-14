//! `ingest/rtmp`: the mixer waits, the publisher dials in.
//!
//! This is the other direction from the core's built in `rtmp/source`, which
//! dials out to somebody else's RTMP server. Here the mixer is the server. It
//! is how a phone, an OBS on the other laptop, or a hardware encoder in the
//! rack reaches a church or a small studio, and today that needs a separate
//! mediamtx running beside the mixer.
//!
//! Two ways to get the bytes:
//!
//! * with no `relay` set, this source owns the listening port itself. One
//!   source, one port, one publisher. This is the whole story for most people.
//! * with `relay` set, the bytes come from `ingest/discover`, which holds one
//!   port for many publishers and hands each one a loopback address.
//!
//! Either way what leaves on stdout is FLV, and the core opens it with
//! `decodebin` exactly as it opens the stream `rtmp/source` dials out for.

use std::io::Read;
use std::sync::atomic::{AtomicBool, AtomicU16, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use godwinmix_sdk::plugin::Reporter;
use godwinmix_sdk::wire::Health;
use serde_json::Value;

use crate::remux::{Out, Remux};
use crate::rtmp::{Event, Filter, Server, Sink};

/// The settings of `ingest/rtmp`.
#[derive(Debug, Clone, PartialEq)]
pub struct Settings {
    pub bind: String,
    pub port: u16,
    pub app: String,
    pub stream_key: String,
    /// `host:port` of an `ingest/discover` relay. Empty means own the port.
    pub relay: String,
}

impl Default for Settings {
    fn default() -> Settings {
        Settings {
            bind: "0.0.0.0".into(),
            // 1935 is the port every encoder offers by default, so a publisher
            // that types nothing but the host still arrives.
            port: 1935,
            // Empty accepts whatever the publisher asks for, which is what
            // "appears as a live source with no configuration" requires.
            app: String::new(),
            stream_key: String::new(),
            relay: String::new(),
        }
    }
}

fn string(params: &Value, key: &str) -> Option<String> {
    params
        .get(key)
        .and_then(Value::as_str)
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

impl Settings {
    pub fn from_params(params: &Value) -> Settings {
        let d = Settings::default();
        Settings {
            bind: string(params, "bind").unwrap_or(d.bind),
            port: params
                .get("port")
                .and_then(Value::as_u64)
                .and_then(|p| u16::try_from(p).ok())
                .unwrap_or(d.port),
            app: string(params, "app").unwrap_or(d.app),
            stream_key: string(params, "stream_key").unwrap_or(d.stream_key),
            relay: string(params, "relay").unwrap_or(d.relay),
        }
    }

    pub fn problem(&self) -> Option<String> {
        if !self.relay.is_empty() && !self.relay.contains(':') {
            return Some(format!(
                "relay is '{}', which is not a host:port. It is filled in by \
                 ingest/discover; leave it empty and this source listens on its own port.",
                self.relay
            ));
        }
        if self.relay.is_empty() && self.bind.is_empty() {
            return Some(
                "bind is empty. Write 0.0.0.0 to listen on every interface, or the address \
                 of the one to listen on."
                    .into(),
            );
        }
        None
    }

    /// The address to give a publisher, for a log line and for `stats`.
    pub fn publish_url(&self, actual_port: u16) -> String {
        let app = if self.app.is_empty() { "live" } else { &self.app };
        let key = if self.stream_key.is_empty() { "<any key>" } else { &self.stream_key };
        format!("rtmp://<this machine>:{actual_port}/{app}/{key}")
    }
}

/// A running `ingest/rtmp`: a listener or a relay reader, and what it has seen.
pub struct Ingest {
    state: Arc<State>,
    /// Held so the listener lives as long as the source does.
    _server: Option<Server>,
    stop: Arc<AtomicBool>,
    reader: Option<std::thread::JoinHandle<()>>,
}

/// What the connection threads write and `health` reads.
struct State {
    /// Who is publishing, or none.
    publisher: Mutex<Option<String>>,
    bytes: AtomicU64,
    /// The port actually bound, which is not known until the listener is up.
    port: AtomicU16,
    /// The address to tell a publisher to use, filled in with the real port.
    where_from: Mutex<String>,
    /// Set when the remuxer has failed. A source whose pipe is broken is not
    /// carrying a picture whatever the publisher thinks.
    broken: AtomicBool,
}

impl State {
    fn new() -> State {
        State {
            publisher: Mutex::new(None),
            bytes: AtomicU64::new(0),
            port: AtomicU16::new(0),
            where_from: Mutex::new(String::new()),
            broken: AtomicBool::new(false),
        }
    }

    fn address(&self) -> String {
        self.where_from.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }
}

impl Ingest {
    /// Start listening, or start reading from a relay.
    pub fn start(
        settings: &Settings,
        reporter: Option<Reporter>,
        out: Out,
    ) -> Result<Ingest, String> {
        let remux = Remux::open(out)?;
        if settings.relay.is_empty() {
            Ingest::listen(settings, reporter, remux)
        } else {
            Ingest::read_relay(settings, reporter, remux)
        }
    }

    fn listen(
        settings: &Settings,
        reporter: Option<Reporter>,
        remux: Remux,
    ) -> Result<Ingest, String> {
        let state = Arc::new(State::new());
        let sink = sink_for(Arc::clone(&state), remux, reporter.clone());
        let filter = Filter {
            app: settings.app.clone(),
            key: settings.stream_key.clone(),
            one_at_a_time: true,
        };
        let server = Server::bind(&settings.bind, settings.port, filter, sink)?;
        let port = server.port();
        state.port.store(port, Ordering::Relaxed);
        *state.where_from.lock().unwrap_or_else(|e| e.into_inner()) = settings.publish_url(port);
        if let Some(r) = &reporter {
            r.info(format!("waiting for a publisher at {}", state.address()));
        }
        Ok(Ingest {
            state,
            _server: Some(server),
            stop: Arc::new(AtomicBool::new(false)),
            reader: None,
        })
    }

    fn read_relay(
        settings: &Settings,
        reporter: Option<Reporter>,
        remux: Remux,
    ) -> Result<Ingest, String> {
        let mut stream = std::net::TcpStream::connect(&settings.relay).map_err(|e| {
            format!(
                "could not reach the ingest relay at {}: {e}. It is opened by \
                 ingest/discover for one publisher and goes away when that publisher \
                 does; call discover again for a current address.",
                settings.relay
            )
        })?;
        let state = Arc::new(State::new());
        *state.publisher.lock().unwrap_or_else(|e| e.into_inner()) = Some(settings.relay.clone());
        *state.where_from.lock().unwrap_or_else(|e| e.into_inner()) =
            format!("the ingest relay at {}", settings.relay);
        let stop = Arc::new(AtomicBool::new(false));
        let reader = {
            let state = Arc::clone(&state);
            let stop = Arc::clone(&stop);
            std::thread::Builder::new()
                .name("gmx-ingest-relay".into())
                .spawn(move || {
                    let mut buffer = vec![0u8; 64 * 1024];
                    while !stop.load(Ordering::Relaxed) {
                        match stream.read(&mut buffer) {
                            Ok(0) => break,
                            Ok(n) => {
                                remux.write(&buffer[..n]);
                                state.bytes.fetch_add(n as u64, Ordering::Relaxed);
                                if remux.broken() {
                                    state.broken.store(true, Ordering::Relaxed);
                                    break;
                                }
                            }
                            Err(_) => break,
                        }
                    }
                    *state.publisher.lock().unwrap_or_else(|e| e.into_inner()) = None;
                    if let Some(r) = &reporter {
                        r.info("the ingest relay closed");
                    }
                })
                .map_err(|e| format!("could not start the relay reader: {e}"))?
        };
        Ok(Ingest { state, _server: None, stop, reader: Some(reader) })
    }

    /// The port actually listening, or 0 when reading from a relay.
    #[cfg(test)]
    pub fn port(&self) -> u16 {
        self.state.port.load(Ordering::Relaxed)
    }

    pub fn health(&self) -> Health {
        if self.state.broken.load(Ordering::Relaxed) {
            return Health::failing(
                "the stream could not be remuxed for the core. The publisher is sending \
                 something this build cannot parse: check its video codec is H.264 and \
                 its audio AAC, which is what RTMP carries.",
            );
        }
        let publisher = self
            .state
            .publisher
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone();
        match publisher {
            Some(who) => {
                let mut health = Health::ok();
                health.detail = Some(format!("{who} is publishing"));
                health
            }
            None => Health::degraded(format!(
                "nobody is publishing. Point an encoder at {} and the picture appears \
                 within a second or two of its first keyframe.",
                self.state.address()
            )),
        }
    }

    pub fn stats(&self) -> Value {
        serde_json::json!({
            "publishing": self.state.publisher.lock().unwrap_or_else(|e| e.into_inner()).clone(),
            "address": self.state.address(),
            "port": self.state.port.load(Ordering::Relaxed),
            "bytes": self.state.bytes.load(Ordering::Relaxed),
        })
    }
}

impl Drop for Ingest {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        self._server = None;
        if let Some(thread) = self.reader.take() {
            let _ = thread.join();
        }
    }
}

/// The sink the RTMP server calls: write the bytes, keep the counters, log.
///
/// A publisher arriving and leaving are the only two things that change this
/// source's health, so the health is pushed at exactly those two moments. The
/// SDK answers the core's `health` call from a cached value, and nothing else
/// here would refresh it; a timer polling a boolean once a second would be work
/// nobody asked for.
fn sink_for(state: Arc<State>, remux: Remux, reporter: Option<Reporter>) -> Sink {
    Arc::new(move |event| match event {
        Event::Arrived { app, key, peer } => {
            let who = format!("{app}/{key} from {peer}");
            *state.publisher.lock().unwrap_or_else(|e| e.into_inner()) = Some(who.clone());
            if let Some(r) = &reporter {
                r.info(format!("{who} started publishing"));
                let mut health = Health::ok();
                health.detail = Some(format!("{who} is publishing"));
                r.health_changed(health);
            }
        }
        Event::Bytes(bytes) => {
            state.bytes.fetch_add(bytes.len() as u64, Ordering::Relaxed);
            remux.write(&bytes);
            if remux.broken() {
                state.broken.store(true, Ordering::Relaxed);
            }
        }
        Event::Left { app, key } => {
            *state.publisher.lock().unwrap_or_else(|e| e.into_inner()) = None;
            if let Some(r) = &reporter {
                r.info(format!("{app}/{key} stopped publishing"));
                r.health_changed(Health::degraded(format!(
                    "{app}/{key} stopped publishing. The port is still open, so the same \
                     encoder reconnecting is picked up without anything being rebuilt."
                )));
            }
        }
        Event::Note(message) => {
            if let Some(r) = &reporter {
                r.warn(message);
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn the_defaults_wait_on_1935_for_any_key() {
        let s = Settings::from_params(&json!({}));
        assert_eq!(s.port, 1935);
        assert!(s.app.is_empty());
        assert!(s.stream_key.is_empty());
        assert!(s.problem().is_none());
        assert!(s.publish_url(1935).contains("<any key>"));
    }

    #[test]
    fn a_configured_app_and_key_show_up_in_the_address_to_hand_out() {
        let s = Settings::from_params(&json!({"app": "live", "stream_key": "phone"}));
        assert_eq!(s.publish_url(1935), "rtmp://<this machine>:1935/live/phone");
    }

    #[test]
    fn a_relay_that_is_not_a_host_port_is_refused_with_the_way_out() {
        let s = Settings::from_params(&json!({"relay": "nonsense"}));
        let problem = s.problem().expect("a relay needs a port");
        assert!(problem.contains("ingest/discover"), "{problem}");
    }

    #[test]
    fn a_listener_on_an_ephemeral_port_comes_up_and_says_nobody_is_publishing() {
        let path = std::env::temp_dir().join(format!("gmx-ingest-{}.flv", std::process::id()));
        let settings = Settings::from_params(&json!({"bind": "127.0.0.1", "port": 0}));
        let ingest = Ingest::start(&settings, None, Out::File(path.clone()))
            .expect("the loopback has a free port");
        assert!(ingest.port() > 0);
        let health = ingest.health();
        assert_eq!(health.state, godwinmix_sdk::wire::HealthState::Degraded);
        assert!(health.detail.unwrap_or_default().contains("rtmp://"));
        assert_eq!(ingest.stats()["bytes"], 0);
        drop(ingest);
        let _ = std::fs::remove_file(&path);
    }

    /// The repository has no `which` crate; this is the same four lines the
    /// core's own tests use.
    fn which(program: &str) -> Option<std::path::PathBuf> {
        let path = std::env::var_os("PATH")?;
        std::env::split_paths(&path)
            .map(|dir| dir.join(program))
            .find(|candidate| candidate.is_file())
    }

    #[test]
    fn a_real_publisher_arrives_and_its_stream_reaches_the_pipe_as_matroska() {
        let Some(launcher) = which("gst-launch-1.0") else {
            eprintln!("skipping: gst-launch-1.0 is not on PATH");
            return;
        };
        if gmx_netkit::init().is_err()
            || !gmx_netkit::elements::exists("rtmp2sink")
            || !gmx_netkit::elements::exists("x264enc")
        {
            eprintln!("skipping: this build of GStreamer cannot publish RTMP");
            return;
        }
        let path = std::env::temp_dir().join(format!("gmx-ingest-live-{}.mkv", std::process::id()));
        let settings = Settings::from_params(&json!({"bind": "127.0.0.1", "port": 0}));
        let ingest =
            Ingest::start(&settings, None, Out::File(path.clone())).expect("the listener starts");
        let port = ingest.port();

        let mut publisher = std::process::Command::new(launcher)
            .args([
                "-q",
                "videotestsrc",
                "is-live=true",
                "!",
                "video/x-raw,width=320,height=240,framerate=30/1",
                "!",
                "x264enc",
                "tune=zerolatency",
                "key-int-max=15",
                "!",
                "h264parse",
                "!",
                "flvmux",
                "streamable=true",
                "!",
                "rtmp2sink",
                &format!("location=rtmp://127.0.0.1:{port}/live/test"),
            ])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .expect("gst-launch-1.0 starts");

        let mut bytes = 0u64;
        let mut publishing = false;
        for _ in 0..100 {
            std::thread::sleep(std::time::Duration::from_millis(200));
            bytes = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
            publishing = ingest.health().state == godwinmix_sdk::wire::HealthState::Ok;
            if bytes > 20_000 && publishing {
                break;
            }
        }
        let stats = ingest.stats();
        let head = std::fs::read(&path).unwrap_or_default();
        let _ = publisher.kill();
        let _ = publisher.wait();
        drop(ingest);
        let _ = std::fs::remove_file(&path);

        assert!(publishing, "the listener never saw a publisher: {stats}");
        assert!(bytes > 20_000, "only {bytes} bytes arrived");
        // The EBML magic. The publisher's FLV is remuxed to Matroska here; the
        // module comment in src/remux.rs says why.
        assert_eq!(
            &head[0..4],
            &[0x1a, 0x45, 0xdf, 0xa3],
            "what came out is not a Matroska stream"
        );
        assert!(stats["publishing"].as_str().unwrap_or("").contains("live/test"));

        // The header being right is not the same as the stream being openable.
        // The core's container transport is `fdsrc ! decodebin`, so this runs
        // the same decodebin over what came out and insists it decodes.
        let kept = std::env::temp_dir().join(format!("gmx-ingest-kept-{}.mkv", std::process::id()));
        std::fs::write(&kept, &head).expect("keep the capture for the decode check");
        let decoded = std::process::Command::new(which("gst-launch-1.0").expect("launcher"))
            .args([
                "-q",
                "filesrc",
                &format!("location={}", kept.display()),
                "!",
                "decodebin",
                "!",
                "fakesink",
            ])
            .output()
            .expect("gst-launch-1.0 runs");
        let _ = std::fs::remove_file(&kept);
        assert!(
            decoded.status.success(),
            "decodebin would not open the stream this plugin produced: {}",
            String::from_utf8_lossy(&decoded.stderr)
        );
    }

    #[test]
    fn a_relay_address_nothing_is_listening_on_names_discover() {
        let path = std::env::temp_dir().join(format!("gmx-ingest-r-{}.flv", std::process::id()));
        let settings = Settings::from_params(&json!({"relay": "127.0.0.1:1"}));
        let err = match Ingest::start(&settings, None, Out::File(path.clone())) {
            Ok(_) => panic!("nothing is listening on port 1"),
            Err(e) => e,
        };
        assert!(err.contains("ingest/discover"), "{err}");
        let _ = std::fs::remove_file(&path);
    }
}
