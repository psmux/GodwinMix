//! What the listener says, acted on.
//!
//! The supervisor's pump hands every `channel.*` event to a queue and goes
//! back to its other plugins; this module's own thread takes them off it.
//! Adding a source waits on the mixer, and nothing a publisher does may hold
//! up the pump that watches every other plugin.

use std::sync::atomic::Ordering;
use std::sync::Arc;

use godwinmix_protocol::types::Event;
use serde_json::{json, Value};
use tracing::{debug, info, warn};

use super::{Channels, Live, PLUGIN, PROVIDE};

pub fn start(channels: &Arc<Channels>) {
    let (tx, rx) = std::sync::mpsc::channel::<(String, Value)>();
    channels.plugins.route_events(
        "channel.",
        Arc::new(move |_, name, params| {
            let _ = tx.send((name.to_string(), params.clone()));
        }),
    );
    let me = channels.clone();
    let started = std::thread::Builder::new().name("channels".into()).spawn(move || {
        me.tidy();
        while let Ok((name, params)) = rx.recv() {
            match name.as_str() {
                "channel.stream" if params["state"] == "idle" => me.went_idle(&params),
                "channel.stream" => me.went_live(&params),
                "channel.refused" => me.refused(&params),
                "channel.destination" => me.destination_report(&params),
                other => debug!(event = other, "a channel event nothing acts on"),
            }
        }
    });
    if let Err(e) = started {
        warn!(?e, "no thread for channel events; streams will not become sources by themselves");
    }
}

impl Channels {
    /// Which channel an event is about: by id, or by application name.
    fn record_for(&self, v: &Value) -> Option<super::Record> {
        let id = v["channel"].as_str().unwrap_or_default();
        let app = v["app"].as_str().unwrap_or_default();
        self.records.lock().iter().find(|r| r.id == id || (!app.is_empty() && r.app == app)).cloned()
    }

    fn note_port(&self, relay: &str) {
        if let Some(port) = relay.rsplit(':').next().and_then(|p| p.parse::<u16>().ok()) {
            self.port.store(port, Ordering::Relaxed);
        }
    }

    pub(super) fn went_live(&self, v: &Value) {
        let Some(record) = self.record_for(v) else { return };
        let Some(mut incoming) = Live::from_plugin(v) else { return };
        incoming.channel = record.id.clone();
        self.note_port(&incoming.relay);
        let fresh = {
            let mut live = self.live.lock();
            match live.iter_mut().find(|l| l.channel == record.id && l.name == incoming.name) {
                Some(known) => {
                    let was_idle = known.state != "live";
                    known.absorb(v);
                    was_idle
                }
                None => {
                    live.push(incoming.clone());
                    true
                }
            }
        };
        if fresh {
            info!(channel = %record.id, stream = %incoming.name, from = %incoming.from, "a stream went live");
            self.hook_stream(&record.id, &incoming.name, "live", Some(&incoming));
            if record.auto_source && record.enabled {
                self.adopt(&record, &incoming.name, &incoming.relay);
            }
        }
        // A plan follows its stream: news of its codecs, size or rate is
        // planned again, and only what changed moves.
        if self.converts(&record.id) {
            self.replan();
        }
        self.announce(&record.id);
    }

    pub(super) fn went_idle(&self, v: &Value) {
        let Some(record) = self.record_for(v) else { return };
        let name = v["stream"].as_str().unwrap_or_default().to_string();
        let source = {
            let mut live = self.live.lock();
            let Some(at) = live.iter().position(|l| l.channel == record.id && l.name == name) else { return };
            let source = live[at].source.clone();
            if source.is_none() {
                live.remove(at);
            }
            source
        };
        info!(channel = %record.id, stream = %name, "a stream left");
        self.hook_stream(&record.id, &name, "idle", None);
        if let Some(source) = source {
            let gone = self.let_go(&source);
            let mut live = self.live.lock();
            if gone {
                live.retain(|l| !(l.channel == record.id && l.name == name));
            } else if let Some(l) = live.iter_mut().find(|l| l.channel == record.id && l.name == name) {
                // A scene holds the source, so it stays and waits for the
                // publisher to come back. The stream shows as idle meanwhile.
                l.state = "idle".into();
                l.video = None;
                l.audio = None;
            }
        }
        if self.converts(&record.id) {
            self.replan();
        }
        self.announce(&record.id);
    }

    fn refused(&self, v: &Value) {
        let asked = v["id"].as_str().unwrap_or_default().to_string();
        let id = self
            .records
            .lock()
            .iter()
            .find(|r| r.id == asked || r.app == asked)
            .map(|r| r.id.clone())
            .unwrap_or(asked);
        self.mixer.emit(Event::ChannelRefused {
            id,
            stream: v["stream"].as_str().unwrap_or_default().to_string(),
            from: v["from"].as_str().unwrap_or_default().to_string(),
            why: v["why"].as_str().unwrap_or_default().to_string(),
        });
    }

    /// Bring the numbers up to date: codec, size, frame rate, bit rate and
    /// dropped GOPs. Asked of the listener when a client asks for a channel,
    /// and never on a timer.
    pub(super) fn refresh(&self) {
        if !self.plugins.is_running(PLUGIN) {
            return;
        }
        let answer = match self.plugins.tool_call(&format!("{PROVIDE}/streams"), json!({})) {
            Ok(answer) => answer,
            Err(e) => {
                debug!(error = %format!("{e:#}"), "the RTMP listener did not answer streams");
                return;
            }
        };
        let body = &answer["structured_content"];
        if let Some(port) = body["port"].as_u64().and_then(|p| u16::try_from(p).ok()) {
            self.port.store(port, Ordering::Relaxed);
        }
        if let Some(rows) = body["destinations"].as_array() {
            self.destination_rates(rows);
        }
        if let Some(rows) = body["listeners"].as_array() {
            self.listener_report(rows);
        }
        let rows = body["streams"].as_array().cloned().unwrap_or_default();
        let mut live = self.live.lock();
        for row in &rows {
            let (app, name) = (row["app"].as_str().unwrap_or(""), row["stream"].as_str().unwrap_or(""));
            if let Some(l) = live.iter_mut().find(|l| l.app == app && l.name == name) {
                l.absorb(row);
            }
        }
    }
}
