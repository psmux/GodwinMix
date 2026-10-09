//! Making, changing and removing channels and their keys.

use godwinmix_protocol::channels::{
    Channel, ChannelAddRequest, ChannelAdded, ChannelKeyAddRequest, ChannelKeyRemoveRequest,
    ChannelRemoved, ChannelSetRequest, KeyAdded,
};
use godwinmix_protocol::error::RpcError;
use godwinmix_protocol::types::Event;

use godwinmix_protocol::channel_ingest::{rtmp_only, ChannelProtocol, Rtmps};

use super::keys::{self, check_app, free, same_app, slug};
use super::newkey::check_secret;
use super::store::Record;
use super::Channels;

impl Channels {
    /// `channel.add`: a channel and its first key.
    pub fn add(&self, req: ChannelAddRequest) -> Result<ChannelAdded, RpcError> {
        let name = req.name.trim().to_string();
        let base = slug(&name);
        if base.is_empty() {
            return Err(RpcError::invalid_params(
                "a channel needs a name with a letter or digit in it, such as Sunday service.",
            )
            .with("field", "name"));
        }
        let app = req.app.map(|a| a.trim().to_string()).unwrap_or_else(|| base.clone());
        check_app(&app)?;
        let typed = req.secret.as_deref().map(check_secret).transpose()?;
        let protocols = self.check_protocols(req.protocols.unwrap_or_else(rtmp_only), Rtmps::default())?;
        let record = {
            let mut records = self.records.lock();
            if let Some(other) = records.iter().find(|r| same_app(&r.app, &app)) {
                return Err(taken(&app, &other.id));
            }
            let id = free(&base, |id| records.iter().any(|r| r.id == id));
            let record = Record {
                id,
                name,
                app,
                enabled: true,
                auto_source: req.auto_source.unwrap_or(true),
                key_mode: req.key_mode.unwrap_or_default(),
                protocols,
                rtmps: Rtmps::default(),
                keys: Vec::new(),
                auto_sources: Vec::new(),
                destinations: Vec::new(),
                extra: Default::default(),
            };
            records.push(record.clone());
            record
        };
        let key = self.make_key(&record.id, None, typed)?;
        self.commit(Some(&record.id))?;
        Ok(ChannelAdded { channel: self.channel(&record.id).expect("just made"), key })
    }

    /// `channel.set`: only what is named moves.
    pub fn set(&self, req: ChannelSetRequest) -> Result<Channel, RpcError> {
        if let Some(app) = &req.app {
            check_app(app.trim())?;
        }
        let (protocols, rtmps) = {
            let records = self.records.lock();
            let current = records.iter().find(|r| r.id == req.id);
            let protocols = req.protocols.clone().or_else(|| current.map(|r| r.protocols.clone())).unwrap_or_else(rtmp_only);
            let rtmps = req.rtmps.or_else(|| current.map(|r| r.rtmps)).unwrap_or_default();
            (protocols, rtmps)
        };
        let protocols = self.check_protocols(protocols, rtmps)?;
        {
            let mut records = self.records.lock();
            if let Some(app) = &req.app {
                if let Some(other) = records.iter().find(|r| same_app(&r.app, app.trim()) && r.id != req.id) {
                    return Err(taken(app.trim(), &other.id));
                }
            }
            let ids: Vec<String> = records.iter().map(|r| r.id.clone()).collect();
            let record = records
                .iter_mut()
                .find(|r| r.id == req.id)
                .ok_or_else(|| RpcError::not_found("channel", &req.id, &ids))?;
            if let Some(name) = req.name.as_ref().map(|n| n.trim()).filter(|n| !n.is_empty()) {
                record.name = name.to_string();
            }
            if let Some(app) = &req.app {
                record.app = app.trim().to_string();
            }
            record.enabled = req.enabled.unwrap_or(record.enabled);
            record.auto_source = req.auto_source.unwrap_or(record.auto_source);
            record.key_mode = req.key_mode.unwrap_or(record.key_mode);
            record.protocols = protocols;
            record.rtmps = rtmps;
        }
        self.commit(Some(&req.id))?;
        if req.auto_source == Some(true) {
            self.adopt_live(&req.id);
        }
        self.get(&req.id)
    }

    /// Streams already live when `auto_source` is switched on become
    /// sources now, rather than at their next reconnect.
    fn adopt_live(&self, id: &str) {
        let Some(record) = self.records.lock().iter().find(|r| r.id == id && r.enabled).cloned() else {
            return;
        };
        let waiting: Vec<(String, String)> = self
            .live
            .lock()
            .iter()
            .filter(|l| l.channel == id && l.state == "live" && l.source.is_none())
            .map(|l| (l.name.clone(), l.relay.clone()))
            .collect();
        for (stream, relay) in waiting {
            self.adopt(&record, &stream, &relay);
        }
        self.announce(id);
    }

    /// `channel.remove`: the channel, its keys, and the sources it added
    /// that no scene holds.
    pub fn remove(&self, id: &str) -> Result<ChannelRemoved, RpcError> {
        let gone = {
            let mut records = self.records.lock();
            let at = records.iter().position(|r| r.id == id);
            at.map(|i| records.remove(i))
        };
        let Some(gone) = gone else { return Err(self.not_found(id)) };
        self.secrets.forget(&keys::scope(id));
        self.forget_channel_sending(id);
        let streams: Vec<super::Live> = {
            let mut live = self.live.lock();
            let (mine, rest): (Vec<_>, Vec<_>) = live.drain(..).partition(|l| l.channel == id);
            *live = rest;
            mine
        };
        for source in gone.auto_sources.iter().chain(streams.iter().filter_map(|l| l.source.as_ref())) {
            self.let_go(source);
        }
        self.commit(None)?;
        self.mixer.emit(Event::ChannelRemoved { id: id.to_string() });
        Ok(ChannelRemoved { removed: id.to_string() })
    }

    /// `channel.key.add`.
    pub fn key_add(&self, req: ChannelKeyAddRequest) -> Result<KeyAdded, RpcError> {
        if !self.records.lock().iter().any(|r| r.id == req.id) {
            return Err(self.not_found(&req.id));
        }
        let typed = req.secret.as_deref().map(check_secret).transpose()?;
        let key = self.make_key(&req.id, req.label, typed)?;
        self.commit(Some(&req.id))?;
        Ok(KeyAdded { key })
    }

    /// `channel.key.remove`. A publisher already on air with it is cut off.
    pub fn key_remove(&self, req: ChannelKeyRemoveRequest) -> Result<Channel, RpcError> {
        {
            let mut records = self.records.lock();
            let ids: Vec<String> = records.iter().map(|r| r.id.clone()).collect();
            let record = records
                .iter_mut()
                .find(|r| r.id == req.id)
                .ok_or_else(|| RpcError::not_found("channel", &req.id, &ids))?;
            let keys: Vec<String> = record.keys.iter().map(|k| k.id.clone()).collect();
            let before = record.keys.len();
            record.keys.retain(|k| k.id != req.key);
            if record.keys.len() == before {
                return Err(RpcError::not_found("key", &req.key, &keys).with("channel", req.id.clone()));
            }
        }
        let _ = self.secrets.set(&keys::scope(&req.id), &req.key, "");
        self.commit(Some(&req.id))?;
        self.get(&req.id)
    }
}

impl Channels {
    /// A channel takes at least one protocol, and RTMPS on a port nothing
    /// else of the mixer's already holds.
    fn check_protocols(&self, mut protocols: Vec<ChannelProtocol>, rtmps: Rtmps) -> Result<Vec<ChannelProtocol>, RpcError> {
        protocols.sort();
        protocols.dedup();
        if protocols.is_empty() && !rtmps.enabled {
            return Err(RpcError::invalid_params(
                "a channel needs at least one way in. Switch on RTMP, SRT, WHIP or RTMPS.",
            )
            .with("field", "protocols"));
        }
        let (rtmp, control) = (self.port.load(std::sync::atomic::Ordering::Relaxed), self.ports.control);
        if rtmps.enabled && (rtmps.port == 0 || rtmps.port == rtmp || rtmps.port == control) {
            return Err(RpcError::invalid_params(format!(
                "RTMPS cannot use port {}: the RTMP port is {rtmp} and this page's own is {control}. \
                 Pick another, such as 443 or 8443.",
                rtmps.port
            ))
            .with("field", "rtmps.port"));
        }
        Ok(protocols)
    }
}

fn taken(app: &str, by: &str) -> RpcError {
    RpcError::invalid_params(format!(
        "the application name '{app}' is already the channel '{by}'. Two channels cannot \
         share one: give this one another name, or change '{by}' first."
    ))
    .with("field", "app")
    .with("channel", by)
}
