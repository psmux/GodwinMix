//! A feed running: what a list says of it, what the show is asked to run,
//! and what the listener and the shows' owner report back.

use std::sync::atomic::Ordering;

use super::*;
use serde_json::Value;

impl Channels {
    /// The feed as a list shows it.
    pub(in crate::channels) fn feed_view(&self, r: &Record) -> ChannelFeed {
        let live = self.live.lock().iter().any(|l| l.channel == r.id && l.state == "live");
        let seen = self.feeds.lock().iter().find(|s| s.channel == r.id).cloned();
        let mut view = ChannelFeed::default();
        match &r.feed {
            StoredFeed::Encoder => {}
            StoredFeed::Programme { show, .. } => {
                view.kind = FeedKind::Programme;
                view.show = show.clone();
                view.output = Some(output_id(&r.id));
            }
            StoredFeed::Pull { uri_host } => {
                view.kind = FeedKind::Pull;
                view.uri_host = Some(uri_host.clone());
                view.state = FeedState::Connecting;
            }
        }
        if !r.enabled {
            view.state = FeedState::Off;
        } else if live {
            view.state = FeedState::Live;
        } else if let Some(s) = seen {
            view.state = s.state;
            view.error = s.error;
            view.retry_in_ms = s.retry_at.map(|at| at.saturating_duration_since(Instant::now()).as_millis() as u64);
        }
        view
    }

    /// Every channel the programme is to be sent into, with the address and
    /// key it publishes with. Reads sealed keys; call it on the blocking pool.
    pub fn programme_feeds(&self) -> Vec<ProgrammeFeed> {
        let records = self.records.lock().clone();
        let port = self.port.load(Ordering::Relaxed);
        records
            .iter()
            .filter(|r| r.enabled)
            .filter_map(|r| {
                let StoredFeed::Programme { show, key } = &r.feed else { return None };
                let secret = self.secrets.get(&keys::scope(&r.id), key)?;
                let stream = match r.key_mode {
                    KeyMode::Query => format!("programme?psk={secret}"),
                    KeyMode::Stream => secret,
                };
                let uri = format!("rtmp://127.0.0.1:{port}/{}/{stream}", keys::in_url(&r.app));
                Some(ProgrammeFeed { channel: r.id.clone(), show: show.clone(), output: output_id(&r.id), uri })
            })
            .collect()
    }

    /// Whether any channel is fed by the programme, so the shows' owner
    /// knows whether there is anything to look after.
    pub fn any_programme_feed(&self) -> bool {
        self.records.lock().iter().any(|r| matches!(r.feed, StoredFeed::Programme { .. }))
    }

    /// What sending the programme into a channel last came to: `None` when
    /// it is in place, the reason in words when it is not. Announced only
    /// when it moved.
    pub fn note_programme(&self, channel: &str, error: Option<String>) {
        let moved = self.note(channel, FeedState::Waiting, error, None);
        if moved {
            self.announce(channel);
        }
    }

    /// `event/channel.pull` from the listener: `{channel, state, error,
    /// retry_in_ms}`.
    pub(in crate::channels) fn pull_report(&self, v: &Value) {
        let Some(id) = v["channel"].as_str() else { return };
        let state = match v["state"].as_str() {
            Some("live") => FeedState::Live,
            Some("retrying") => FeedState::Reconnecting,
            _ => FeedState::Connecting,
        };
        let error = v["error"].as_str().map(str::to_string);
        let retry = v["retry_in_ms"].as_u64().map(Duration::from_millis);
        if self.note(id, state, error, retry) {
            self.announce(id);
        }
    }

    fn note(&self, channel: &str, state: FeedState, error: Option<String>, retry: Option<Duration>) -> bool {
        let mut feeds = self.feeds.lock();
        let retry_at = retry.map(|d| Instant::now() + d);
        match feeds.iter_mut().find(|s| s.channel == channel) {
            Some(s) if s.state == state && s.error == error && retry.is_none() => false,
            Some(s) => {
                (s.state, s.error, s.retry_at) = (state, error, retry_at);
                true
            }
            None => {
                feeds.push(Seen { channel: channel.to_string(), state, error, retry_at });
                true
            }
        }
    }

    /// Whether a channel's live stream may become a mixer source by itself.
    /// The programme fed back into the mixer would show itself.
    pub(in crate::channels) fn adoptable(&self, r: &Record) -> bool {
        r.auto_source && r.enabled && !matches!(r.feed, StoredFeed::Programme { .. })
    }
}
