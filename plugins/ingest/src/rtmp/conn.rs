//! One client's connection, from the handshake to the last tag.

use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use rml_rtmp::handshake::{Handshake, HandshakeProcessResult, PeerType};
use rml_rtmp::sessions::{
    ServerSession, ServerSessionConfig, ServerSessionEvent, ServerSessionResult,
};

use super::{Gate, Inlet};
use crate::codec;
use crate::media_tag::{MediaTag, TagKind};

/// The version byte every RTMP handshake starts with.
const RTMP_VERSION: u8 = 3;

/// Serve one accepted client until it goes.
pub fn serve(stream: TcpStream, gate: Arc<dyn Gate>, stop: &AtomicBool) {
    let peer = stream.peer_addr().ok();
    let mut conn = Connection {
        stream,
        peer: peer.map(|a| a.to_string()).unwrap_or_else(|| "unknown".into()),
        loopback: peer.is_some_and(|a| a.ip().is_loopback()),
        gate,
        inlets: HashMap::new(),
    };
    if let Err(e) = conn.run(stop) {
        conn.gate.note(format!("an RTMP connection from {} ended: {e}", conn.peer));
    }
    // Dropping the inlets is how the gate learns each publisher has left.
    conn.inlets.clear();
}

struct Connection {
    stream: TcpStream,
    peer: String,
    loopback: bool,
    gate: Arc<dyn Gate>,
    /// What each published stream feeds, by the name it was published under.
    inlets: HashMap<String, Box<dyn Inlet>>,
}

impl Connection {
    fn run(&mut self, stop: &AtomicBool) -> Result<(), String> {
        let mut handshake = Handshake::new(PeerType::Server);
        let mut buffer = vec![0u8; 64 * 1024];
        let mut session: Option<ServerSession> = None;
        let mut first = true;

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
            let bytes = &buffer[..read];
            if std::mem::take(&mut first) && bytes[0] != RTMP_VERSION && self.loopback {
                // Not RTMP, from this machine: a reader asking the hub for a
                // stream. It gets the socket and this thread becomes its writer.
                let client = self.stream.try_clone().map_err(|e| e.to_string())?;
                self.gate.relay(client, bytes);
                return Ok(());
            }
            if let Some(session) = session.as_mut() {
                let results = session
                    .handle_input(bytes)
                    .map_err(|e| format!("the publisher sent something unreadable: {e:?}"))?;
                self.act(session, results)?;
                continue;
            }
            match handshake.process_bytes(bytes) {
                Ok(HandshakeProcessResult::InProgress { response_bytes }) => {
                    self.write(&response_bytes)?;
                }
                Ok(HandshakeProcessResult::Completed { response_bytes, remaining_bytes }) => {
                    self.write(&response_bytes)?;
                    let (mut new, results) = ServerSession::new(ServerSessionConfig::new())
                        .map_err(|e| format!("the RTMP session would not start: {e:?}"))?;
                    self.act(&mut new, results)?;
                    if !remaining_bytes.is_empty() {
                        let results = new
                            .handle_input(&remaining_bytes)
                            .map_err(|e| format!("the publisher sent something unreadable: {e:?}"))?;
                        self.act(&mut new, results)?;
                    }
                    session = Some(new);
                }
                Err(e) => {
                    return Err(format!(
                        "the RTMP handshake failed: {e:?}. The client may not be speaking \
                         RTMP at all; check the address it was given."
                    ))
                }
            }
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

    fn event(&mut self, session: &mut ServerSession, event: ServerSessionEvent) -> Result<(), String> {
        match event {
            ServerSessionEvent::ConnectionRequested { request_id, app_name } => {
                let packets = match self.gate.connect(&app_name) {
                    Ok(()) => session.accept_request(request_id),
                    Err(why) => {
                        self.gate.note(format!("refused a connection from {} to '{app_name}': {why}", self.peer));
                        session.reject_request(request_id, "NetConnection.Connect.Rejected", &why)
                    }
                };
                self.send(packets.map_err(|e| format!("could not answer the connection: {e:?}"))?)
            }
            ServerSessionEvent::PublishStreamRequested { request_id, app_name, stream_key, .. } => {
                let packets = match self.gate.admit(&app_name, &stream_key, &self.peer, self.kick()) {
                    Ok(inlet) => {
                        self.inlets.insert(stream_key, inlet);
                        session.accept_request(request_id)
                    }
                    Err(why) => session.reject_request(request_id, "NetStream.Publish.Denied", &why),
                };
                self.send(packets.map_err(|e| format!("could not answer the publisher: {e:?}"))?)
            }
            ServerSessionEvent::VideoDataReceived { stream_key, data, timestamp, .. } => {
                self.push(&stream_key, TagKind::Video, timestamp.value, &data);
                Ok(())
            }
            ServerSessionEvent::AudioDataReceived { stream_key, data, timestamp, .. } => {
                self.push(&stream_key, TagKind::Audio, timestamp.value, &data);
                Ok(())
            }
            ServerSessionEvent::StreamMetadataChanged { stream_key, metadata, .. } => {
                let body = crate::flv::metadata_body(&metadata);
                self.push(&stream_key, TagKind::Script, 0, &body);
                Ok(())
            }
            ServerSessionEvent::PublishStreamFinished { stream_key, .. } => {
                self.inlets.remove(&stream_key);
                Ok(())
            }
            _ => Ok(()),
        }
    }

    /// One message, as a tag, to whoever this stream feeds. The one copy of
    /// the payload this plugin makes is here.
    fn push(&mut self, stream_key: &str, kind: TagKind, timestamp_ms: u32, body: &[u8]) {
        let Some(inlet) = self.inlets.get_mut(stream_key) else { return };
        inlet.tag(MediaTag {
            kind,
            timestamp_ms,
            keyframe: kind == TagKind::Video && codec::is_keyframe(body),
            sequence_header: codec::is_sequence_header(kind, body),
            payload: Arc::from(body),
        });
    }

    /// A way to end this connection from another thread.
    fn kick(&self) -> super::Kick {
        let socket = self.stream.try_clone().ok();
        Arc::new(move || {
            if let Some(s) = &socket {
                let _ = s.shutdown(std::net::Shutdown::Both);
            }
        })
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
