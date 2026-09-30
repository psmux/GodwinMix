//! Channels in a project file, out and back in.
//!
//! Out: each channel's record as the channels file keeps it, less the sources
//! it added at runtime, and its keys and destination addresses only when the
//! export asked for secrets. In: records placed straight into the table, with
//! what the file carried sealed in the secret store. `channel.add` cannot do
//! this, because it always makes a new key, and a project whose encoders stop
//! working after a restore is not a restore.

use std::collections::BTreeMap;

use godwinmix_protocol::error::RpcError;
use serde_json::{json, Value};

use super::keys::{self, free};
use super::store::Record;
use super::Channels;

/// One channel as an import would place it.
pub struct Incoming {
    pub record: Record,
    /// Key id to secret, when the file carried them.
    pub keys: BTreeMap<String, String>,
    /// Destination id to address and key, when the file carried them.
    pub destinations: BTreeMap<String, (String, Option<String>)>,
}

impl Channels {
    /// Every channel as a project carries it.
    pub fn project_export(&self, secrets: bool) -> Vec<Value> {
        let records = self.records.lock().clone();
        records
            .iter()
            .map(|r| {
                let mut kept = r.clone();
                kept.auto_sources.clear();
                let mut value = serde_json::to_value(&kept).unwrap_or(Value::Null);
                if secrets {
                    value["secrets"] = self.secrets_of(r);
                }
                value
            })
            .collect()
    }

    fn secrets_of(&self, r: &Record) -> Value {
        let scope = keys::scope(&r.id);
        let keys: BTreeMap<String, String> = r
            .keys
            .iter()
            .filter_map(|k| self.secrets.get(&scope, &k.id).filter(|s| !s.is_empty()).map(|s| (k.id.clone(), s)))
            .collect();
        let destinations: BTreeMap<String, Value> = self
            .stored(r)
            .into_iter()
            .map(|d| (d.id.clone(), json!({"server": d.server, "key": d.key})))
            .collect();
        json!({"keys": keys, "destinations": destinations})
    }

    /// The ids and application names in use now.
    pub fn project_ids(&self) -> Vec<(String, String)> {
        self.records.lock().iter().map(|r| (r.id.clone(), r.app.clone())).collect()
    }

    /// Put channels from a project in. `replace` takes out every channel the
    /// file does not have first. Answers what a person still has to do.
    pub fn project_import(&self, incoming: Vec<Incoming>, replace: bool) -> Result<Vec<String>, RpcError> {
        let mut waiting = Vec::new();
        if replace {
            let gone: Vec<String> = self
                .project_ids()
                .into_iter()
                .map(|(id, _)| id)
                .filter(|id| !incoming.iter().any(|i| &i.record.id == id))
                .collect();
            for id in gone {
                self.remove(&id)?;
            }
        }
        for one in incoming {
            waiting.extend(self.place(one)?);
        }
        self.commit(None)?;
        for (id, _) in self.project_ids() {
            self.announce(&id);
        }
        Ok(waiting)
    }

    /// One channel in, over an existing one of the same id or beside the rest.
    fn place(&self, one: Incoming) -> Result<Vec<String>, RpcError> {
        let Incoming { mut record, keys: secrets, destinations } = one;
        let mut waiting = Vec::new();
        let before = self.records.lock().iter().find(|r| r.id == record.id).cloned();
        record.auto_sources.clear();
        let scope = keys::scope(&record.id);
        match (&before, secrets.is_empty()) {
            // Same channel, no keys in the file: the keys this mixer holds stay.
            (Some(old), true) => record.keys = old.keys.clone(),
            _ => {
                record.keys.retain(|k| secrets.contains_key(&k.id));
                for k in &record.keys {
                    self.secrets
                        .set(&scope, &k.id, &secrets[&k.id])
                        .map_err(|e| RpcError::internal(format!("sealing a key: {e:#}")))?;
                }
            }
        }
        for d in record.destinations.iter_mut() {
            let sealed_here = before.as_ref().is_some_and(|b| b.destinations.iter().any(|o| o.id == d.id));
            match destinations.get(&d.id) {
                Some((server, key)) => self.seal_one(&record.id, &d.id, server, key.clone())?,
                None if sealed_here => {}
                None => {
                    d.enabled = false;
                    d.has_key = false;
                    waiting.push(format!(
                        "channel {} sends to {} once its address and key are given again in Channels",
                        record.id, d.label
                    ));
                }
            }
        }
        let id = record.id.clone();
        let needs_key = record.keys.is_empty();
        {
            let mut records = self.records.lock();
            records.retain(|r| r.id != id);
            records.push(record);
        }
        if needs_key {
            let key = self.make_key(&id, Some("Key 1".into()))?;
            waiting.push(format!(
                "channel {id} has a new key ({}) because the file carried none: give it to its encoders",
                key.label
            ));
        }
        Ok(waiting)
    }
}

/// Read a channel from a project file, renamed where `taken` says its id or
/// application name is in use. Answers the channel and its old id when it
/// was renamed.
pub fn read(value: &Value, taken: &dyn Fn(&str, &str) -> bool) -> Result<(Incoming, Option<String>), String> {
    let mut plain = value.clone();
    let secrets = plain.as_object_mut().and_then(|o| o.remove("secrets")).unwrap_or(Value::Null);
    let mut record: Record = serde_json::from_value(plain).map_err(|e| format!("a channel in the file is damaged: {e}"))?;
    keys::check_app(&record.app).map_err(|e| e.message)?;
    let was = record.id.clone();
    let renamed = taken(&record.id, "id") || taken(&record.app, "app");
    if renamed {
        record.id = free(&record.id, |id| taken(id, "id"));
        record.app = free(&record.app, |app| taken(app, "app"));
    }
    let keys = serde_json::from_value(secrets["keys"].clone()).unwrap_or_default();
    let destinations = secrets["destinations"]
        .as_object()
        .map(|o| {
            o.iter()
                .map(|(id, d)| {
                    let server = d["server"].as_str().unwrap_or_default().to_string();
                    let key = d["key"].as_str().map(str::to_string);
                    (id.clone(), (server, key))
                })
                .collect()
        })
        .unwrap_or_default();
    Ok((Incoming { record, keys, destinations }, renamed.then_some(was)))
}
