//! A channel as a client sees it: the record, its keys as hints, where to
//! publish, and what is live on it.

use std::sync::atomic::Ordering;

use godwinmix_protocol::channels::{Channel, ChannelKey, ChannelPublish, KeyMode};

use super::{net, Channels, Live, Record};

impl Channels {
    pub(super) fn channel(&self, id: &str) -> Option<Channel> {
        let record = self.records.lock().iter().find(|r| r.id == id).cloned()?;
        Some(self.view(&record))
    }

    pub(super) fn view(&self, r: &Record) -> Channel {
        let port = self.port.load(Ordering::Relaxed);
        let server = format!("rtmp://{}:{port}/{}", net::first_address(), r.app);
        let example = match r.key_mode {
            KeyMode::Query => format!("{server}/main?psk=<key>"),
            KeyMode::Stream => format!("{server}/<key>"),
        };
        let streams = self.live.lock().iter().filter(|l| l.channel == r.id).map(Live::view).collect();
        Channel {
            id: r.id.clone(),
            name: r.name.clone(),
            app: r.app.clone(),
            enabled: r.enabled,
            auto_source: r.auto_source,
            key_mode: r.key_mode,
            keys: r
                .keys
                .iter()
                .map(|k| ChannelKey { id: k.id.clone(), label: k.label.clone(), created: k.created.clone(), hint: k.hint.clone() })
                .collect(),
            publish: ChannelPublish { server, example },
            streams,
            destinations: Vec::new(),
        }
    }
}
