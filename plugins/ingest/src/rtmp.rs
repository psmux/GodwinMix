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
//! Accepts a publisher, filters by application name and stream key, and turns
//! the published audio and video messages into FLV on a sink. One publisher at
//! a time per listener: a second one is refused with a message saying why,
//! because a source is one picture and silently switching it would be worse.

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, AtomicU16, Ordering};
use std::sync::Arc;

use rml_rtmp::handshake::{Handshake, HandshakeProcessResult, PeerType};
use rml_rtmp::sessions::{
    ServerSession, ServerSessionConfig, ServerSessionEvent, ServerSessionResult,
};

use crate::flv;

/// What happens to a publisher's stream.
#[derive(Debug, Clone)]
pub enum Event {
    /// A publisher was accepted. The bytes that follow are one FLV stream.
    Arrived { app: String, key: String, peer: String },
    /// FLV bytes, starting with the file header.
    Bytes(Vec<u8>),
    /// The publisher went away. Any bytes after this belong to a new stream.
    Left { app: String, key: String },
    /// Something worth a log line: a refusal, a protocol error.
    Note(String),
}

/// Where the events go. Called from a connection thread, so it must not block.
pub type Sink = Arc<dyn Fn(Event) + Send + Sync>;

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
    /// The legible id a source gets when it is added for this publisher. Slugs,
    /// never UUIDs: `live/phone` becomes `live-phone`.
    pub fn slug(&self) -> String {
        let raw = format!("{}-{}", self.app, self.key);
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
}

/// What a listener accepts.
#[derive(Debug, Clone, Default)]
pub struct Filter {
    /// The RTMP application name, the first path segment. Empty accepts any.
    pub app: String,
    /// The stream key, the rest of the path. Empty accepts any.
    pub key: String,
    /// Refuse a second publisher while one is live.
    pub one_at_a_time: bool,
}

impl Filter {
    fn accepts(&self, app: &str, key: &str) -> bool {
        (self.app.is_empty() || self.app == app) && (self.key.is_empty() || self.key == key)
    }

    /// Why a publisher was refused, for the message sent back to it.
    fn refusal(&self, app: &str, key: &str) -> String {
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
    pub fn bind(bind: &str, port: u16, filter: Filter, sink: Sink) -> Result<Server, String> {
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
        // A short accept timeout is what lets the thread notice `stop` without
        // a second socket to wake it up.
        listener
            .set_nonblocking(false)
            .map_err(|e| format!("could not configure the listener: {e}"))?;

        let stop = Arc::new(AtomicBool::new(false));
        let thread = spawn_accept(listener, filter, sink, Arc::clone(&stop));
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
    filter: Filter,
    sink: Sink,
    stop: Arc<AtomicBool>,
) -> std::thread::JoinHandle<()> {
    let live = Arc::new(AtomicU16::new(0));
    std::thread::Builder::new()
        .name("gmx-rtmp-accept".into())
        .spawn(move || {
            for incoming in listener.incoming() {
                if stop.load(Ordering::Relaxed) {
                    return;
                }
                let Ok(stream) = incoming else { continue };
                let filter = filter.clone();
                let sink = Arc::clone(&sink);
                let live = Arc::clone(&live);
                let stop = Arc::clone(&stop);
                let _ = std::thread::Builder::new()
                    .name("gmx-rtmp-conn".into())
                    .spawn(move || {
                        let peer = stream
                            .peer_addr()
                            .map(|a| a.to_string())
                            .unwrap_or_else(|_| "unknown".into());
                        let mut conn = Connection::new(stream, peer, filter, sink, live);
                        if let Err(e) = conn.run(&stop) {
                            conn.note(format!("an RTMP connection ended: {e}"));
                        }
                        conn.finish();
                    });
            }
        })
        .expect("could not start the RTMP accept thread")
}

/// One publisher's connection, from the handshake to the last tag.
struct Connection {
    stream: TcpStream,
    peer: String,
    filter: Filter,
    sink: Sink,
    live: Arc<AtomicU16>,
    /// Set once this connection owns the listener's one publisher slot.
    holding: Option<Publisher>,
    /// Bytes are only forwarded once a keyframe has been seen, so a decoder is
    /// never handed a run of inter frames with nothing to decode them against.
    seen_keyframe: bool,
}

impl Connection {
    fn new(
        stream: TcpStream,
        peer: String,
        filter: Filter,
        sink: Sink,
        live: Arc<AtomicU16>,
    ) -> Connection {
        Connection {
            stream,
            peer,
            filter,
            sink,
            live,
            holding: None,
            seen_keyframe: false,
        }
    }

    fn note(&self, message: String) {
        (self.sink)(Event::Note(message));
    }

    fn run(&mut self, stop: &Arc<AtomicBool>) -> Result<(), String> {
        let mut handshake = Handshake::new(PeerType::Server);
        let mut buffer = [0u8; 8192];
        let mut session: Option<ServerSession> = None;

        loop {
            if stop.load(Ordering::Relaxed) {
                return Ok(());
            }
            let read = self
                .stream
                .read(&mut buffer)
                .map_err(|e| format!("reading from the publisher failed: {e}"))?;
            if read == 0 {
                return Ok(());
            }
            let mut bytes = &buffer[..read];

            if session.is_none() {
                match handshake.process_bytes(bytes) {
                    Ok(HandshakeProcessResult::InProgress { response_bytes }) => {
                        self.write(&response_bytes)?;
                        continue;
                    }
                    Ok(HandshakeProcessResult::Completed { response_bytes, remaining_bytes }) => {
                        self.write(&response_bytes)?;
                        let config = ServerSessionConfig::new();
                        let (new, results) = ServerSession::new(config)
                            .map_err(|e| format!("the RTMP session would not start: {e:?}"))?;
                        session = Some(new);
                        self.act(session.as_mut().expect("just set"), results)?;
                        if remaining_bytes.is_empty() {
                            continue;
                        }
                        let owned = remaining_bytes;
                        let results = session
                            .as_mut()
                            .expect("just set")
                            .handle_input(&owned)
                            .map_err(|e| format!("the publisher sent something unreadable: {e:?}"))?;
                        self.act(session.as_mut().expect("just set"), results)?;
                        continue;
                    }
                    Err(e) => {
                        return Err(format!(
                            "the RTMP handshake failed: {e:?}. The client may not be speaking \
                             RTMP at all; check the address it was given."
                        ))
                    }
                }
            }

            let session = session.as_mut().expect("a session exists past the handshake");
            let results = session
                .handle_input(bytes)
                .map_err(|e| format!("the publisher sent something unreadable: {e:?}"))?;
            bytes = &[];
            let _ = bytes;
            self.act(session, results)?;
        }
    }

    /// Do what one batch of session results asks for.
    fn act(
        &mut self,
        session: &mut ServerSession,
        results: Vec<ServerSessionResult>,
    ) -> Result<(), String> {
        for result in results {
            match result {
                ServerSessionResult::OutboundResponse(packet) => self.write(&packet.bytes)?,
                ServerSessionResult::RaisedEvent(event) => self.event(session, event)?,
                ServerSessionResult::UnhandleableMessageReceived(_) => {}
            }
        }
        Ok(())
    }

    fn event(
        &mut self,
        session: &mut ServerSession,
        event: ServerSessionEvent,
    ) -> Result<(), String> {
        match event {
            ServerSessionEvent::ConnectionRequested { request_id, app_name } => {
                if !self.filter.app.is_empty() && self.filter.app != app_name {
                    let why = self.filter.refusal(&app_name, "");
                    self.note(format!("refused a publisher on '{app_name}': {why}"));
                    let packets = session
                        .reject_request(request_id, "NetConnection.Connect.Rejected", &why)
                        .map_err(|e| format!("could not refuse the connection: {e:?}"))?;
                    return self.send(packets);
                }
                let packets = session
                    .accept_request(request_id)
                    .map_err(|e| format!("could not accept the connection: {e:?}"))?;
                self.send(packets)
            }
            ServerSessionEvent::PublishStreamRequested {
                request_id,
                app_name,
                stream_key,
                ..
            } => self.publish_requested(session, request_id, app_name, stream_key),
            ServerSessionEvent::VideoDataReceived { data, timestamp, .. } => {
                if self.holding.is_none() {
                    return Ok(());
                }
                if !self.seen_keyframe {
                    // The AVC sequence header (packet type 0) is what a decoder
                    // needs first, and it arrives marked as a keyframe.
                    if !flv::is_keyframe(&data) {
                        return Ok(());
                    }
                    self.seen_keyframe = true;
                }
                (self.sink)(Event::Bytes(flv::video(timestamp.value, &data)));
                Ok(())
            }
            ServerSessionEvent::AudioDataReceived { data, timestamp, .. } => {
                if self.holding.is_none() || !self.seen_keyframe {
                    return Ok(());
                }
                (self.sink)(Event::Bytes(flv::audio(timestamp.value, &data)));
                Ok(())
            }
            ServerSessionEvent::PublishStreamFinished { .. } => {
                self.release();
                Ok(())
            }
            _ => Ok(()),
        }
    }

    fn publish_requested(
        &mut self,
        session: &mut ServerSession,
        request_id: u32,
        app_name: String,
        stream_key: String,
    ) -> Result<(), String> {
        if !self.filter.accepts(&app_name, &stream_key) {
            let why = self.filter.refusal(&app_name, &stream_key);
            self.note(format!("refused '{app_name}/{stream_key}': {why}"));
            let packets = session
                .reject_request(request_id, "NetStream.Publish.Denied", &why)
                .map_err(|e| format!("could not refuse the publisher: {e:?}"))?;
            return self.send(packets);
        }
        if self.filter.one_at_a_time
            && self
                .live
                .compare_exchange(0, 1, Ordering::AcqRel, Ordering::Acquire)
                .is_err()
        {
            let why = "this source already has a publisher. One RTMP source carries one \
                       picture; add a second ingest/rtmp source on another port, or run \
                       ingest/discover, which takes many publishers on one port."
                .to_string();
            self.note(format!("refused '{app_name}/{stream_key}': {why}"));
            let packets = session
                .reject_request(request_id, "NetStream.Publish.Denied", &why)
                .map_err(|e| format!("could not refuse the publisher: {e:?}"))?;
            return self.send(packets);
        }
        if !self.filter.one_at_a_time {
            self.live.fetch_add(1, Ordering::AcqRel);
        }

        let packets = session
            .accept_request(request_id)
            .map_err(|e| format!("could not accept the publisher: {e:?}"))?;
        self.send(packets)?;

        let publisher = Publisher {
            app: app_name.clone(),
            key: stream_key.clone(),
            peer: self.peer.clone(),
        };
        self.holding = Some(publisher);
        self.seen_keyframe = false;
        (self.sink)(Event::Arrived {
            app: app_name,
            key: stream_key,
            peer: self.peer.clone(),
        });
        (self.sink)(Event::Bytes(flv::header()));
        Ok(())
    }

    /// Give up the publisher slot and say so, once.
    fn release(&mut self) {
        let Some(publisher) = self.holding.take() else { return };
        self.live.fetch_sub(1, Ordering::AcqRel);
        (self.sink)(Event::Left { app: publisher.app, key: publisher.key });
    }

    fn finish(&mut self) {
        self.release();
    }

    fn send(&mut self, packets: Vec<ServerSessionResult>) -> Result<(), String> {
        for packet in packets {
            if let ServerSessionResult::OutboundResponse(packet) = packet {
                self.write(&packet.bytes)?;
            }
        }
        Ok(())
    }

    fn write(&mut self, bytes: &[u8]) -> Result<(), String> {
        self.stream
            .write_all(bytes)
            .map_err(|e| format!("writing to the publisher failed: {e}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_filter_with_nothing_set_takes_anything() {
        let f = Filter::default();
        assert!(f.accepts("live", "phone"));
        assert!(f.accepts("", ""));
    }

    #[test]
    fn a_filter_on_the_application_refuses_another_and_says_where_to_publish() {
        let f = Filter { app: "live".into(), ..Default::default() };
        assert!(f.accepts("live", "anything"));
        assert!(!f.accepts("stream", "anything"));
        let why = f.refusal("stream", "phone");
        assert!(why.contains("rtmp://<host>:<port>/live/"), "{why}");
    }

    #[test]
    fn a_filter_on_the_key_names_the_key_it_wanted() {
        let f = Filter { key: "phone".into(), ..Default::default() };
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
        // An id must start with a letter, so one that would not is prefixed
        // rather than being handed to the core to refuse.
        assert_eq!(p("", "2024").slug(), "rtmp-2024");
    }

    #[test]
    fn a_server_binds_an_ephemeral_port_and_gives_it_back() {
        let sink: Sink = Arc::new(|_| {});
        let server = Server::bind("127.0.0.1", 0, Filter::default(), sink)
            .expect("the loopback has a free port");
        assert!(server.port() > 0);
    }

    #[test]
    fn binding_a_port_that_is_taken_names_the_usual_cause() {
        let sink: Sink = Arc::new(|_| {});
        let first = Server::bind("127.0.0.1", 0, Filter::default(), Arc::clone(&sink))
            .expect("the first bind works");
        let err = match Server::bind("127.0.0.1", first.port(), Filter::default(), sink) {
            Ok(_) => panic!("two servers must not share one port"),
            Err(e) => e,
        };
        assert!(err.contains("mediamtx"), "{err}");
    }

    #[test]
    fn a_connection_that_is_not_rtmp_is_refused_without_taking_the_server_down() {
        use std::io::Write as _;
        let sink: Sink = Arc::new(|_| {});
        let server = Server::bind("127.0.0.1", 0, Filter::default(), sink).expect("bind");
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
