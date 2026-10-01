//! `channel:<app>/<stream>`: a stream someone publishes to a channel, read
//! straight off the hub. The tags are already the hub's, so nothing is
//! demuxed or parsed; each is a pointer handed on.
//!
//! When the publisher leaves, the reader is told and subscribes again, and
//! the hub hands it the next publisher from its first keyframe.

use std::time::{Duration, Instant};

use serde_json::json;

use super::super::{Input, Sink, StopSignal};
use super::outlet::Shared;
use super::spec::{InputError, InputSpec};
use super::stats::{InputStats, State};
use crate::hub::{Hub, Recv};

pub struct Channel {
    hub: Hub,
    app: String,
    stream: String,
}

impl Channel {
    pub fn new(spec: &InputSpec, hub: Option<&Hub>) -> Result<Channel, InputError> {
        let name = spec.uri.split_once(':').map(|(_, n)| n.trim_start_matches('/')).unwrap_or("");
        let (app, stream) = name.split_once('/').filter(|(a, s)| !a.is_empty() && !s.is_empty()).ok_or_else(|| {
            InputError::new(
                format!("'{}' names no stream. Write channel:<app>/<stream>, as the channel's page shows it, for example channel:church/main.", spec.uri),
                json!({"field": "uri", "got": spec.uri}),
            )
        })?;
        let hub = hub.cloned().ok_or_else(|| {
            InputError::new("this host has no channel server to read from. Add a channel first, then point the show at it.", json!({"field": "uri"}))
        })?;
        Ok(Channel { hub, app: app.to_string(), stream: stream.to_string() })
    }

    fn name(&self) -> String {
        format!("channel:{}/{}", self.app, self.stream)
    }
}

impl Input for Channel {
    fn run(self: Box<Self>, sink: Sink, stop: StopSignal) {
        let out = Shared::new(sink);
        let mut stats = InputStats::default();
        let mut tick = Instant::now();
        while !stop.is_stopped() {
            let reader = self.hub.subscribe(&self.app, &self.stream);
            out.new_session();
            loop {
                if stop.is_stopped() {
                    return;
                }
                match reader.recv_timeout(Duration::from_millis(250)) {
                    Recv::Tag(tag) => out.tag(tag),
                    Recv::Ended => {
                        (stats.state, stats.error) = (State::Retrying, Some(format!("the publisher of {} left; waiting for the next", self.name())));
                        out.publish(&mut stats);
                        break;
                    }
                    Recv::Timeout => {}
                }
                if tick.elapsed() >= Duration::from_secs(1) {
                    tick = Instant::now();
                    let quiet = out.quiet_ms();
                    (stats.state, stats.error) = match quiet {
                        Some(q) if q < 3_000 => (State::Live, None),
                        Some(_) => (State::Retrying, Some(format!("nothing from {} for a while", self.name()))),
                        None => (State::Connecting, Some(format!("nobody is publishing to {} yet", self.name()))),
                    };
                    out.publish(&mut stats);
                }
            }
        }
    }
}
