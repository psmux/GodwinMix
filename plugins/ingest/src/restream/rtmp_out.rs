//! Publishing to an RTMP or RTMPS server with `rml_rtmp`'s client session.
//!
//! The session is sans-io, as the server half in `rtmp.rs` is: it turns
//! calls into bytes and bytes into events, and this module owns the socket.
//! A tag's payload goes out as the body of an RTMP message unchanged.

use std::io::{Read, Write};
use std::time::{Duration, Instant};

use bytes::Bytes;
use rml_rtmp::handshake::{Handshake, HandshakeProcessResult, PeerType};
use rml_rtmp::sessions::{
    ClientSession, ClientSessionConfig, ClientSessionEvent as Ev, ClientSessionResult,
    PublishRequestType,
};
use rml_rtmp::time::RtmpTimestamp;

use crate::media_tag::{MediaTag, TagKind};

use super::io::{timed_out, Io};
use super::link::{Failure, Link};
use super::target::{RtmpUrl, Target};

/// How long the whole connect, handshake and publish dance may take.
const DANCE: Duration = Duration::from_secs(10);

pub struct RtmpLink {
    io: Io,
    session: ClientSession,
    name: String,
    buf: Vec<u8>,
}

impl RtmpLink {
    pub fn dial(target: &Target, url: RtmpUrl) -> Result<RtmpLink, Failure> {
        let name = target.name();
        let mut io = Io::open(&url)?;
        let lost = |what: &str| Failure::Lost(format!("{name} {what}"));
        let rest = handshake(&mut io).map_err(|e| lost(&format!("did not speak RTMP: {e}")))?;
        let mut config = ClientSessionConfig::new();
        config.tc_url = Some(url.tc_url());
        let (session, _) = ClientSession::new(config)
            .map_err(|e| lost(&format!("could not start a session: {e:?}")))?;
        let mut link = RtmpLink { io, session, name, buf: vec![0; 16 * 1024] };
        link.publish(&url, rest)?;
        link.io.read_wait(Duration::from_millis(1));
        Ok(link)
    }

    /// connect, createStream, publish, and wait for the server to say yes.
    fn publish(&mut self, url: &RtmpUrl, mut pending: Vec<u8>) -> Result<(), Failure> {
        let first = self.session.request_connection(url.app.clone()).map_err(|e| self.lost(e))?;
        self.write_results(vec![first])?;
        let started = Instant::now();
        let mut asked = false;
        loop {
            if started.elapsed() > DANCE {
                return Err(self.lost("did not answer in time"));
            }
            if pending.is_empty() {
                pending = self.read_some().map_err(|e| self.refusal(asked, url, e))?;
            }
            let results = self.session.handle_input(&pending).map_err(|e| {
                if asked { self.refused_key(&format!("{e:?}")) } else { self.lost(e) }
            })?;
            pending.clear();
            for result in results {
                match result {
                    ClientSessionResult::OutboundResponse(p) => self.write(&p.bytes)?,
                    ClientSessionResult::RaisedEvent(event) => match event {
                        Ev::ConnectionRequestAccepted => {
                            let r = self
                                .session
                                .request_publishing(url.stream.clone(), PublishRequestType::Live)
                                .map_err(|e| self.lost(e))?;
                            self.write_results(vec![r])?;
                            asked = true;
                        }
                        Ev::ConnectionRequestRejected { description } => {
                            return Err(Failure::Refused(format!(
                                "{} refused the application '{}': {description}",
                                self.name, url.app
                            )))
                        }
                        Ev::PublishRequestAccepted => return Ok(()),
                        Ev::UnknownTransactionResultReceived { .. } if asked => {
                            return Err(self.refused_key("the publish was turned down"))
                        }
                        Ev::UnhandleableOnStatusCode { code } if code.contains("Publish") => {
                            return Err(self.refused_key(&code))
                        }
                        _ => {}
                    },
                    ClientSessionResult::UnhandleableMessageReceived(_) => {}
                }
            }
        }
    }

    fn read_some(&mut self) -> Result<Vec<u8>, String> {
        match self.io.read(&mut self.buf) {
            Ok(0) => Err("closed the connection".into()),
            Ok(n) => Ok(self.buf[..n].to_vec()),
            Err(e) => Err(e.to_string()),
        }
    }

    /// A connection that ends while a publish is being asked for is how
    /// YouTube and Twitch say no to a key: they hang up without a word.
    fn refusal(&self, asked: bool, url: &RtmpUrl, e: String) -> Failure {
        if asked {
            self.refused_key(&e)
        } else {
            Failure::Lost(format!("{} at {} {e}", self.name, url.tc_url()))
        }
    }

    fn refused_key(&self, detail: &str) -> Failure {
        Failure::Refused(format!(
            "{} refused the key ({detail}). Copy the stream key again from the platform and \
             paste it into this destination",
            self.name
        ))
    }

    fn lost(&self, e: impl std::fmt::Debug) -> Failure {
        Failure::Lost(format!("{} broke off the session: {e:?}", self.name))
    }

    fn write_results(&mut self, results: Vec<ClientSessionResult>) -> Result<usize, Failure> {
        let mut n = 0;
        for r in results {
            if let ClientSessionResult::OutboundResponse(p) = r {
                self.write(&p.bytes)?;
                n += p.bytes.len();
            }
        }
        Ok(n)
    }

    fn write(&mut self, bytes: &[u8]) -> Result<(), Failure> {
        self.io.write_all(bytes).and_then(|_| self.io.flush()).map_err(|e| {
            Failure::Lost(if timed_out(&e) {
                format!("{} stopped taking the stream for ten seconds", self.name)
            } else {
                format!("{} closed the connection ({e})", self.name)
            })
        })
    }
}

impl Link for RtmpLink {
    fn send(&mut self, tag: &MediaTag, timestamp_ms: u32) -> Result<usize, Failure> {
        let at = RtmpTimestamp::new(timestamp_ms);
        let payload = Bytes::from_owner(tag.payload.clone());
        let result = match tag.kind {
            TagKind::Video => self.session.publish_video_data(payload, at, false),
            TagKind::Audio => self.session.publish_audio_data(payload, at, false),
            TagKind::Script => match super::meta::parse(&tag.payload) {
                Some(m) => self.session.publish_metadata(&m),
                None => return Ok(0),
            },
        };
        let packet = result.map_err(|e| self.lost(e))?;
        self.write_results(vec![packet])
    }

    fn poll(&mut self) -> Result<(), Failure> {
        let bytes = match self.io.read(&mut self.buf) {
            Ok(0) => return Err(Failure::Lost(format!("{} closed the connection", self.name))),
            Ok(n) => self.buf[..n].to_vec(),
            Err(e) if timed_out(&e) => return Ok(()),
            Err(e) => return Err(Failure::Lost(format!("{} closed the connection ({e})", self.name))),
        };
        let results = self.session.handle_input(&bytes).map_err(|e| self.lost(e))?;
        self.write_results(results).map(|_| ())
    }

    fn close(&mut self) {
        if let Ok(results) = self.session.stop_publishing() {
            let _ = self.write_results(results);
        }
        self.io.shutdown();
    }
}

/// The client side of the RTMP handshake. Answers the bytes that arrived
/// after it, which belong to the session.
fn handshake(io: &mut Io) -> Result<Vec<u8>, String> {
    let mut hs = Handshake::new(PeerType::Client);
    let p0p1 = hs.generate_outbound_p0_and_p1().map_err(|e| format!("{e:?}"))?;
    io.write_all(&p0p1).map_err(|e| e.to_string())?;
    let mut buf = [0u8; 4096];
    loop {
        let n = io.read(&mut buf).map_err(|e| e.to_string())?;
        if n == 0 {
            return Err("the connection closed during the handshake".into());
        }
        match hs.process_bytes(&buf[..n]).map_err(|e| format!("{e:?}"))? {
            HandshakeProcessResult::InProgress { response_bytes } => {
                io.write_all(&response_bytes).map_err(|e| e.to_string())?
            }
            HandshakeProcessResult::Completed { response_bytes, remaining_bytes } => {
                io.write_all(&response_bytes).map_err(|e| e.to_string())?;
                return Ok(remaining_bytes);
            }
        }
    }
}
