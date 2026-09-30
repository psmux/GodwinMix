//! Making, changing and removing channels and their keys.

use godwinmix_protocol::channels::{
    Channel, ChannelAddRequest, ChannelAdded, ChannelKeyAddRequest, ChannelKeyRemoveRequest,
    ChannelRemoved, ChannelSetRequest, KeyAdded, NewKey,
};
use godwinmix_protocol::error::RpcError;
use godwinmix_protocol::types::Event;

use super::keys::{self, check_app, free, slug};
use super::store::{KeyRecord, Record};
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
        let record = {
            let mut records = self.records.lock();
            if let Some(other) = records.iter().find(|r| r.app == app) {
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
                keys: Vec::new(),
                auto_sources: Vec::new(),
                destinations: Vec::new(),
                extra: Default::default(),
            };
            records.push(record.clone());
            record
        };
        let key = self.make_key(&record.id, None)?;
        self.commit(Some(&record.id))?;
        Ok(ChannelAdded { channel: self.channel(&record.id).expect("just made"), key })
    }

    /// `channel.set`: only what is named moves.
    pub fn set(&self, req: ChannelSetRequest) -> Result<Channel, RpcError> {
        if let Some(app) = &req.app {
            check_app(app.trim())?;
        }
        {
            let mut records = self.records.lock();
            if let Some(app) = &req.app {
                if let Some(other) = records.iter().find(|r| r.app == app.trim() && r.id != req.id) {
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
        let key = self.make_key(&req.id, req.label)?;
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

    /// Make a key, seal it, and add its record. Answers with the secret, which
    /// `channel.key.reveal` can read back later from the store.
    fn make_key(&self, channel: &str, label: Option<String>) -> Result<NewKey, RpcError> {
        let secret = godwinmix_core::secrets::random_key(keys::KEY_LEN)
            .map_err(|e| RpcError::internal(format!("making a key: {e:#}")))?;
        let mut records = self.records.lock();
        let ids: Vec<String> = records.iter().map(|r| r.id.clone()).collect();
        let record = records
            .iter_mut()
            .find(|r| r.id == channel)
            .ok_or_else(|| RpcError::not_found("channel", channel, &ids))?;
        let label = label
            .map(|l| l.trim().to_string())
            .filter(|l| !l.is_empty())
            .unwrap_or_else(|| format!("Key {}", record.keys.len() + 1));
        let base = match slug(&label) {
            s if s.is_empty() => "key".to_string(),
            s => s,
        };
        let id = free(&base, |id| record.keys.iter().any(|k| k.id == id));
        self.secrets
            .set(&keys::scope(channel), &id, &secret)
            .map_err(|e| RpcError::internal(format!("sealing the key: {e:#}")))?;
        record.keys.push(KeyRecord { id: id.clone(), label: label.clone(), created: keys::now(), hint: keys::hint(&secret) });
        Ok(NewKey { id, label, secret })
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
