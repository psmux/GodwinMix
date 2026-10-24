//! Where a pull's tags go: the channel's stream on the hub, published when
//! the first tag comes and ended when the input stops being live, so the
//! channel shows live only while a picture arrives. The core is told on the
//! input's own thread, once a second, never on a streaming thread.

use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use serde_json::{json, Value};

use super::{Want, STREAM};
use crate::direct::input::stats::State;
use crate::direct::{InputStats, TagSink};
use crate::gate::ChannelGate;
use crate::hub::Publication;
use crate::media_tag::MediaTag;

/// How soon the same state may be said again in other words.
const SAY_AGAIN: Duration = Duration::from_secs(5);

pub struct ToChannel {
    gate: Arc<ChannelGate>,
    channel: String,
    app: String,
    /// What a message may show of the address: scheme, host and port.
    shown: String,
    /// The address as given and as the input writes it, taken out of every
    /// message: either may carry a key.
    hidden: [String; 2],
    publication: Arc<Mutex<Option<Publication>>>,
    refused: Arc<Mutex<Option<String>>>,
    said: Option<(State, Option<String>, Instant)>,
    announced: Option<Value>,
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

impl ToChannel {
    pub fn new(gate: Arc<ChannelGate>, want: &Want) -> ToChannel {
        let shown = godwinmix_protocol::destination::uri_host(&want.url);
        let hidden = [crate::direct::input::redact(&want.url), want.url.clone()];
        ToChannel {
            gate,
            channel: want.channel.clone(),
            app: want.app.clone(),
            shown,
            hidden,
            publication: Default::default(),
            refused: Default::default(),
            said: None,
            announced: None,
        }
    }

    /// A message with the address cut down to what may be shown.
    fn clean(&self, text: &str) -> String {
        self.hidden.iter().filter(|h| !h.is_empty()).fold(text.to_string(), |t, h| t.replace(h.as_str(), &self.shown))
    }

    /// The address would not open at all: said once, and nothing runs.
    pub fn refused(mut self, why: &str) {
        let why = self.clean(why);
        self.say(State::Retrying, Some(why), None);
    }

    fn say(&mut self, state: State, error: Option<String>, retry_ms: Option<u64>) {
        let same = self.said.as_ref().is_some_and(|(s, e, at)| *s == state && (*e == error || at.elapsed() < SAY_AGAIN));
        if same && retry_ms.is_none() {
            return;
        }
        self.said = Some((state, error.clone(), Instant::now()));
        let Some(r) = &self.gate.reporter else { return };
        let mut v = json!({"channel": self.channel, "state": state.as_str(), "from": self.shown});
        if let Some(e) = error {
            v["error"] = json!(e);
        }
        if let Some(ms) = retry_ms {
            v["retry_in_ms"] = json!(ms);
        }
        r.event("channel.pull", v);
    }

    /// `channel.stream` live, and again whenever what is known of it moves.
    fn announce(&mut self) {
        let Some(mut described) = lock(&self.publication).as_ref().and_then(|p| p.describe()) else { return };
        let gist = json!([described.get("video"), described.get("audio")]);
        if self.announced.as_ref() == Some(&gist) {
            return;
        }
        self.announced = Some(gist);
        described["channel"] = json!(self.channel);
        described["relay"] = json!(self.gate.relay_address());
        if let Some(r) = &self.gate.reporter {
            r.event("channel.stream", described);
        }
    }

    /// The stream ends for every reader, and the core is told.
    fn end(&mut self) {
        if lock(&self.publication).take().is_none() {
            return;
        }
        self.announced = None;
        if let Some(r) = &self.gate.reporter {
            r.event("channel.stream", json!({"channel": self.channel, "app": self.app, "stream": STREAM, "state": "idle"}));
        }
    }
}

impl TagSink for ToChannel {
    /// On a streaming thread: publish at the first tag, then hand each on.
    fn tag(&mut self, tag: MediaTag) {
        let mut publication = lock(&self.publication);
        if publication.is_none() {
            match self.gate.hub.publish_via(&self.app, STREAM, &self.shown, None, "pull") {
                Ok(p) => *publication = Some(p),
                Err(why) => {
                    *lock(&self.refused) = Some(why);
                    return;
                }
            }
        }
        if let Some(p) = publication.as_ref() {
            p.push(tag);
        }
    }

    /// On the input's own thread, about once a second.
    fn stats(&mut self, stats: &InputStats) {
        let refused = lock(&self.refused).take();
        // The input's meter outlives a connection, so the first second of a
        // new one can read as live from the last one's frames. Live is a
        // stream on the hub, nothing less.
        let state = match stats.state {
            State::Live if lock(&self.publication).is_none() => State::Connecting,
            other => other,
        };
        if state == State::Live {
            self.announce();
        } else {
            self.end();
        }
        let error = match refused {
            Some(why) => Some(format!("somebody else is publishing to the channel, so the pull waits: {why}")),
            None => stats.error.as_deref().map(|e| self.clean(e)),
        };
        self.say(state, error, stats.retry_ms);
    }
}

impl Drop for ToChannel {
    fn drop(&mut self) {
        self.end();
    }
}
