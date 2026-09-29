//! An RTMP server, in the plugin, in Rust.
//!
//! # Why not mediamtx
//!
//! The roadmap gives two options for the RTMP listener: a Rust RTMP crate, or
//! `mediamtx` bundled as a sidecar. This is the crate.
//!
//! `rml_rtmp` 0.8.0 (MIT) is sans-io: it parses chunks and raises events and
//! never touches a socket, so the whole of the networking here is
//! `std::net::TcpListener` and one thread per connection. It is about 6,000
//! lines of library code, and it brings `byteorder`, `rml_amf0`, and a second
//! copy of the `sha2` and `hmac` stack (0.9 and 0.10, against the 0.10 already
//! in the tree) for the FP9 handshake. That is eight small pure Rust crates and
//! no C.
//!
//! `mediamtx` is a 30 MB Go binary per platform, a second process to supervise,
//! a YAML configuration to keep in step, its own ports, and a download and
//! signing story for five platforms. It is also the thing the roadmap says this
//! plugin exists to remove: "today it needs a separate mediamtx". Bundling it
//! would have made the mixer heavier than the tool it replaces, on a project
//! whose first constraint is running on a Raspberry Pi.
//!
//! The one thing `mediamtx` gives that this does not is Enhanced RTMP (HEVC and
//! AV1 over RTMP). If that becomes the thing people need, `rtmpx` is the crate
//! to look at again; today it wants Rust 1.97 and the workspace is on 1.82.
//!
//! # What this does
//!
//! Accepts publishers and asks a [`Gate`] about each one. The gate decides who
//! may publish and where the tags go: `ingest/rtmp` on its own port lets one
//! publisher at a time into a remuxer, and the channel server lets many
//! publishers on many channels into the hub. The connection itself only turns
//! RTMP messages into [`MediaTag`]s, one allocation per message, and never
//! waits on anything but its own socket.
//!
//! The same port also answers a loopback reader asking for a stream by name,
//! which is how a mixer source in another process reads the hub. See
//! `src/relay.rs`.

use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use crate::media_tag::MediaTag;

mod conn;

/// Where one publisher's tags go. Dropping it means the publisher has left.
pub trait Inlet: Send {
    fn tag(&mut self, tag: MediaTag);
}

/// Who may publish, and where their tags go. Called from connection threads,
/// so nothing in it may block for long.
pub trait Gate: Send + Sync {
    /// A client asked to connect to an application. `app` may still carry a
    /// query string.
    fn connect(&self, _app: &str) -> Result<(), String> {
        Ok(())
    }

    /// A publisher asked for `app` and `stream`, exactly as sent (a key may
    /// ride on either as a query string). `Err` is the sentence it is refused
    /// with.
    fn admit(&self, app: &str, stream: &str, peer: &str) -> Result<Box<dyn Inlet>, String>;

    /// A loopback client that is not speaking RTMP. The gate that serves the
    /// hub takes it; any other lets it close.
    fn relay(&self, _client: TcpStream, _first: &[u8]) {}

    /// Something worth a log line: a refusal, a protocol error.
    fn note(&self, message: String);
}

/// A listening RTMP server.
pub struct Server {
    port: u16,
    stop: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}

/// One publisher, as `discover` and the tool report it.
#[derive(Debug, Clone, PartialEq)]
pub struct Publisher {
    pub app: String,
    pub key: String,
    pub peer: String,
}

impl Publisher {
    /// The legible id a source gets when it is added for this publisher.
    pub fn slug(&self) -> String {
        slug(&format!("{}-{}", self.app, self.key))
    }
}

/// Slugs, never UUIDs: `live/phone` becomes `live-phone`. An id must start
/// with a letter, so one that would not is prefixed rather than being handed
/// to the core to refuse.
pub fn slug(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut last_dash = true;
    for c in raw.chars() {
        let c = c.to_ascii_lowercase();
        if c.is_ascii_alphanumeric() {
            out.push(c);
            last_dash = false;
        } else if !last_dash {
            out.push('-');
            last_dash = true;
        }
    }
    let trimmed = out.trim_matches('-').to_string();
    if trimmed.is_empty() || !trimmed.starts_with(|c: char| c.is_ascii_alphabetic()) {
        format!("rtmp-{trimmed}")
    } else {
        trimmed
    }
}

/// What a listener on its own port accepts.
#[derive(Debug, Clone, Default)]
pub struct Filter {
    /// The RTMP application name, the first path segment. Empty accepts any.
    pub app: String,
    /// The stream key, the rest of the path. Empty accepts any.
    pub key: String,
}

impl Filter {
    pub fn accepts(&self, app: &str, key: &str) -> bool {
        (self.app.is_empty() || self.app == app) && (self.key.is_empty() || self.key == key)
    }

    /// Why a publisher was refused, for the message sent back to it.
    pub fn refusal(&self, app: &str, key: &str) -> String {
        if !self.app.is_empty() && self.app != app {
            return format!(
                "this mixer is listening for the application '{}', not '{app}'. \
                 Publish to rtmp://<host>:<port>/{}/<key>.",
                self.app, self.app
            );
        }
        format!(
            "this mixer is listening for the stream key '{}', not '{key}'.",
            self.key
        )
    }
}

impl Server {
    /// Bind and start accepting. `port` may be 0, in which case the operating
    /// system picks one and [`Server::port`] says which.
    pub fn bind(bind: &str, port: u16, gate: Arc<dyn Gate>) -> Result<Server, String> {
        let listener = TcpListener::bind((bind, port)).map_err(|e| {
            format!(
                "could not listen for RTMP on {bind}:{port}: {e}. Another process has the \
                 port (a mediamtx or nginx-rtmp left running is the usual one), or the \
                 port is below 1024 and this process is not allowed to bind it."
            )
        })?;
        let port = listener
            .local_addr()
            .map(|a| a.port())
            .map_err(|e| format!("the listener has no address: {e}"))?;
        let stop = Arc::new(AtomicBool::new(false));
        let thread = spawn_accept(listener, gate, Arc::clone(&stop));
        Ok(Server { port, stop, thread: Some(thread) })
    }

    /// The port actually bound.
    pub fn port(&self) -> u16 {
        self.port
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        // Connect to our own port so the blocking accept returns at once.
        let _ = TcpStream::connect(("127.0.0.1", self.port));
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn spawn_accept(
    listener: TcpListener,
    gate: Arc<dyn Gate>,
    stop: Arc<AtomicBool>,
) -> std::thread::JoinHandle<()> {
    std::thread::Builder::new()
        .name("gmx-rtmp-accept".into())
        .spawn(move || {
            for incoming in listener.incoming() {
                if stop.load(Ordering::Relaxed) {
                    return;
                }
                let Ok(stream) = incoming else { continue };
                let gate = Arc::clone(&gate);
                let stop = Arc::clone(&stop);
                let _ = std::thread::Builder::new()
                    .name("gmx-rtmp-conn".into())
                    .spawn(move || conn::serve(stream, gate, &stop));
            }
        })
        .expect("could not start the RTMP accept thread")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A gate that lets nobody in.
    struct Closed;

    impl Gate for Closed {
        fn admit(&self, _: &str, _: &str, _: &str) -> Result<Box<dyn Inlet>, String> {
            Err("closed".into())
        }
        fn note(&self, _: String) {}
    }

    #[test]
    fn a_filter_with_nothing_set_takes_anything() {
        let f = Filter::default();
        assert!(f.accepts("live", "phone"));
        assert!(f.accepts("", ""));
    }

    #[test]
    fn a_filter_on_the_application_refuses_another_and_says_where_to_publish() {
        let f = Filter { app: "live".into(), key: String::new() };
        assert!(f.accepts("live", "anything"));
        assert!(!f.accepts("stream", "anything"));
        let why = f.refusal("stream", "phone");
        assert!(why.contains("rtmp://<host>:<port>/live/"), "{why}");
    }

    #[test]
    fn a_filter_on_the_key_names_the_key_it_wanted() {
        let f = Filter { app: String::new(), key: "phone".into() };
        assert!(f.accepts("live", "phone"));
        assert!(!f.accepts("live", "laptop"));
        assert!(f.refusal("live", "laptop").contains("'phone'"));
    }

    #[test]
    fn a_publisher_becomes_a_legible_slug() {
        let p = |app: &str, key: &str| Publisher {
            app: app.into(),
            key: key.into(),
            peer: "1.2.3.4:1".into(),
        };
        assert_eq!(p("live", "phone").slug(), "live-phone");
        assert_eq!(p("live", "Studio Cam 1").slug(), "live-studio-cam-1");
        assert_eq!(p("live", "a/b?c=d").slug(), "live-a-b-c-d");
        assert_eq!(p("", "2024").slug(), "rtmp-2024");
    }

    #[test]
    fn a_server_binds_an_ephemeral_port_and_gives_it_back() {
        let server = Server::bind("127.0.0.1", 0, Arc::new(Closed))
            .expect("the loopback has a free port");
        assert!(server.port() > 0);
    }

    #[test]
    fn binding_a_port_that_is_taken_names_the_usual_cause() {
        let first = Server::bind("127.0.0.1", 0, Arc::new(Closed)).expect("the first bind works");
        let err = match Server::bind("127.0.0.1", first.port(), Arc::new(Closed)) {
            Ok(_) => panic!("two servers must not share one port"),
            Err(e) => e,
        };
        assert!(err.contains("mediamtx"), "{err}");
    }

    #[test]
    fn a_connection_that_is_not_rtmp_is_refused_without_taking_the_server_down() {
        use std::io::Write as _;
        let server = Server::bind("127.0.0.1", 0, Arc::new(Closed)).expect("bind");
        let mut client =
            TcpStream::connect(("127.0.0.1", server.port())).expect("the server accepts");
        // A plain HTTP request is the usual wrong thing to send at 1935.
        client.write_all(b"GET / HTTP/1.1\r\nHost: x\r\n\r\n").expect("write");
        drop(client);
        std::thread::sleep(std::time::Duration::from_millis(200));
        // Still listening: a second connection is accepted.
        assert!(TcpStream::connect(("127.0.0.1", server.port())).is_ok());
    }
}
