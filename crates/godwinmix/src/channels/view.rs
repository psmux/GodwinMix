//! A channel as a client sees it: the record, its keys as hints, where to
//! publish over each protocol it has on, and what is live on it.

use std::sync::atomic::Ordering;

use godwinmix_protocol::channels::{Channel, ChannelKey, ChannelProtocol, ChannelPublish, KeyMode, PublishAddress};

use super::{keys, net, Channels, Live, Record};

impl Channels {
    pub(super) fn channel(&self, id: &str) -> Option<Channel> {
        let record = self.records.lock().iter().find(|r| r.id == id).cloned()?;
        Some(self.view(&record))
    }

    pub(super) fn view(&self, r: &Record) -> Channel {
        let addresses = self.addresses(r, &net::first_address());
        let server = format!("rtmp://{}:{}/{}", net::first_address(), self.port.load(Ordering::Relaxed), keys::in_url(&r.app));
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
            protocols: r.protocols.clone(),
            rtmps: r.rtmps,
            keys: r
                .keys
                .iter()
                .map(|k| ChannelKey {
                    id: k.id.clone(),
                    label: k.label.clone(),
                    created: k.created.clone(),
                    hint: k.hint.clone(),
                    imported: k.imported,
                })
                .collect(),
            publish: ChannelPublish { server, example, addresses },
            streams,
            destinations: self.destination_views(r),
        }
    }

    /// Where an encoder is pointed for each protocol the channel has on.
    fn addresses(&self, r: &Record, host: &str) -> Vec<PublishAddress> {
        let (app, rtmp) = (keys::in_url(&r.app), self.port.load(Ordering::Relaxed));
        let by_name = r.key_mode == KeyMode::Stream;
        let rtmp_like = |scheme: &str, port: u16| {
            let server = format!("{scheme}://{host}:{port}/{app}");
            let example = if by_name { format!("{server}/<key>") } else { format!("{server}/main?psk=<key>") };
            PublishAddress { protocol: scheme.into(), server, example }
        };
        let mut out = Vec::new();
        if r.protocols.contains(&ChannelProtocol::Rtmp) {
            out.push(rtmp_like("rtmp", rtmp));
        }
        if r.rtmps.enabled {
            out.push(rtmp_like("rtmps", r.rtmps.port));
        }
        if r.protocols.contains(&ChannelProtocol::Srt) {
            let server = format!("srt://{host}:{}", self.ports.srt);
            let example = if by_name {
                format!("{server}?streamid={app}/<key>")
            } else {
                format!("{server}?streamid={app}/main&passphrase=<key>")
            };
            out.push(PublishAddress { protocol: "srt".into(), server, example });
        }
        if r.protocols.contains(&ChannelProtocol::Whip) {
            // The key is the bearer token, or the last part of the path on a
            // channel whose key is the stream name.
            let server = format!("http://{host}:{}/whip/{app}", self.ports.control);
            let example = if by_name { format!("{server}/<key>") } else { format!("{server}/main") };
            out.push(PublishAddress { protocol: "whip".into(), server, example });
        }
        out
    }
}
