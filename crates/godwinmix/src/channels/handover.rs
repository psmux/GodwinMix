//! The table the listener is handed: every channel, its keys, its
//! destinations and, for a destination that asked for a rendition, what to
//! build for it.
//!
//! The plan is made again every time the table is, from the records and the
//! live streams as they are, so the plan follows the stream: a publisher
//! that changes size mid stream is planned again when the listener says so,
//! and only the nodes whose work changed move.

use godwinmix_govern::Governor;
use godwinmix_protocol::rendition::PlanView;
use godwinmix_protocol::types::Event;
use serde_json::{json, Value};
use tracing::warn;

use super::{keys, Channels, Record, PLUGIN};

impl Channels {
    /// Give the listener the table: at its next start, and now if `now`.
    pub(super) fn hand_over(&self, now: bool) {
        let table = self.table();
        self.send(table, now);
    }

    /// Plan again and hand the listener the table, only if it changed. Every
    /// channel with a rendition is announced when it did, so a page sees the
    /// new plan. Called from the channels' own threads, never a handler's.
    pub(super) fn replan(&self) {
        let table = self.table();
        if self.handed.lock().as_ref() == Some(&table) {
            return;
        }
        self.send(table, true);
        let ids: Vec<String> = self
            .records
            .lock()
            .iter()
            .filter(|r| r.destinations.iter().any(|d| d.rendition.is_some()))
            .map(|r| r.id.clone())
            .collect();
        for id in ids {
            self.announce(&id);
        }
    }

    /// Whether any channel's destination asked for a rendition, which is
    /// when a stream coming, changing or going needs a new plan.
    pub(super) fn converts(&self, channel: &str) -> bool {
        self.records.lock().iter().any(|r| r.id == channel && r.destinations.iter().any(|d| d.rendition.is_some()))
    }

    /// Count channel transcodes against the station's governor, the one the
    /// programme's renditions use. The graph work calls this when it wires the
    /// governor into the station; until then the channels keep their own.
    pub fn use_governor(&self, governor: Governor) -> bool {
        self.transcode.use_governor(governor)
    }

    /// One channel's plan, for `rendition.plan {scope: "channel:<id>"}`:
    /// `None` for a channel that converts nothing. The `rendition.plan`
    /// handler (the graph work's) answers a `channel:` scope with this.
    pub fn rendition_plan(&self, channel: &str) -> Option<PlanView> {
        self.transcode.plan_view(channel)
    }

    /// `event/rendition.plan` with scope `channel:<id>` for every channel
    /// that converts something.
    fn announce_plans(&self) {
        let ids: Vec<String> = self.records.lock().iter().map(|r| r.id.clone()).collect();
        for id in ids {
            if let Some(plan) = self.transcode.plan_view(&id) {
                self.mixer.emit(Event::RenditionPlan { scope: format!("channel:{id}"), plan });
            }
        }
    }

    fn table(&self) -> Value {
        let records = self.records.lock().clone();
        let live = self.live.lock().clone();
        self.transcode.replan(&records, &live, |r| self.stored(r));
        Value::Array(records.iter().map(|r| self.row(r)).collect())
    }

    fn row(&self, r: &Record) -> Value {
        let keys: Vec<Value> = r
            .keys
            .iter()
            .filter_map(|k| {
                let secret = self.secrets.get(&keys::scope(&r.id), &k.id)?;
                Some(json!({"id": k.id, "secret": secret}))
            })
            .collect();
        let mut row = json!({
            "id": r.id,
            "app": r.app,
            "enabled": r.enabled,
            "key_mode": r.key_mode,
            "keys": keys,
            "protocols": r.protocols,
            "destinations": self.destination_table(r),
        });
        // TOML has no null, so an RTMPS that is off is left out, and so is a
        // channel's `transcode` when it converts nothing.
        if r.rtmps.enabled {
            row["rtmps_port"] = json!(r.rtmps.port);
        }
        if let Some(streams) = self.transcode.streams(&r.id) {
            row["transcode"] = streams;
        }
        row
    }

    fn send(&self, table: Value, now: bool) {
        self.hand_over_tls();
        match toml::Value::try_from(&table) {
            Ok(value) => self.plugins().set_extra(PLUGIN, "channels", Some(value)),
            Err(e) => warn!(%e, "the channel table would not convert for the plugin"),
        }
        *self.handed.lock() = Some(table);
        if now {
            for (instance, answer) in self.plugins().configure_plugin(PLUGIN) {
                if let Err(e) = answer {
                    warn!(%instance, error = %format!("{e:#}"), "the channel server did not take the new channel table");
                }
            }
            self.announce_plans();
        }
        self.watch_renditions();
    }
}
